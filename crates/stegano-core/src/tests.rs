//! End-to-end tests over every carrier format, using synthetic carriers.

use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use flacenc::error::Verify;

use image::{DynamicImage, ImageBuffer, ImageFormat};
use rand::{RngExt, SeedableRng, rngs::StdRng};

use super::*;

const PASS: &str = "correct horse battery staple";

fn payload() -> Vec<Entry> {
    vec![
        Entry::text("The eagle lands at midnight. 🦅"),
        Entry::file(
            "plan.txt",
            b"step one: ...\n".repeat(20),
            Some(1_700_000_000),
        ),
        Entry::file("key.bin", (0..=255u8).collect(), None),
    ]
}

/// Smooth structure plus sensor-like noise, roughly like a photo.
fn photo(width: u32, height: u32, channels: usize, seed: u64) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = Vec::with_capacity((width * height) as usize * channels);
    for y in 0..height {
        for x in 0..width {
            for c in 0..channels {
                let base =
                    128.0 + 70.0 * ((x as f64 / 17.0 + c as f64).sin() * (y as f64 / 23.0).cos());
                let noise = rng.random_range(-12.0..12.0);
                out.push((base + noise).clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

fn encode_image(img: DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn rgb_png() -> Vec<u8> {
    let img = ImageBuffer::from_raw(320, 240, photo(320, 240, 3, 1)).unwrap();
    encode_image(DynamicImage::ImageRgb8(img), ImageFormat::Png)
}

fn roundtrip(carrier: &[u8], format: CarrierFormat) -> Embedded {
    let entries = payload();
    let out = embed(carrier, &entries, PASS, &EmbedOptions::default()).unwrap();
    assert_eq!(out.format, format);
    assert_eq!(CarrierFormat::detect(&out.data).unwrap(), format);
    assert_eq!(extract(&out.data, PASS).unwrap(), entries);
    assert!(matches!(
        extract(&out.data, "wrong passphrase"),
        Err(Error::NotFound)
    ));
    out
}

#[test]
fn png_rgb() {
    let out = roundtrip(&rgb_png(), CarrierFormat::Png);
    // ~2.7 kB of payload in 230k elements: STC should touch only a small
    // fraction of the elements a naive LSB tool would (half the payload bits).
    let naive = out.report.embedded_bytes * 8 / 2;
    assert!(
        out.report.changed * 2 < naive,
        "changed {} vs naive {naive}",
        out.report.changed
    );
}

#[test]
fn png_rgba_skips_transparent_pixels() {
    let mut px = photo(256, 256, 4, 2);
    for (i, p) in px.chunks_mut(4).enumerate() {
        p[3] = if (i % 256) < 128 { 0 } else { 255 };
    }
    let img = DynamicImage::ImageRgba8(ImageBuffer::from_raw(256, 256, px.clone()).unwrap());
    let out = roundtrip(&encode_image(img, ImageFormat::Png), CarrierFormat::Png);
    let stego = image::load_from_memory(&out.data)
        .unwrap()
        .to_rgba8()
        .into_raw();
    for (a, b) in px.chunks(4).zip(stego.chunks(4)) {
        assert_eq!(a[3], b[3], "alpha must never change");
        if a[3] == 0 {
            assert_eq!(a, b, "transparent pixels must not change");
        }
    }
}

#[test]
fn png_16_bit_grey() {
    let px: Vec<u16> = photo(200, 200, 1, 3)
        .iter()
        .map(|&v| v as u16 * 257 + (v as u16 % 7))
        .collect();
    let img = DynamicImage::ImageLuma16(ImageBuffer::from_raw(200, 200, px).unwrap());
    roundtrip(&encode_image(img, ImageFormat::Png), CarrierFormat::Png);
}

#[test]
fn png_keeps_ancillary_chunks() {
    let original = rgb_png();
    // Insert a tEXt chunk right after IHDR (8-byte signature + 25-byte IHDR).
    let text = b"Comment\0holiday photo";
    let mut chunk = (text.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(text);
    chunk.extend_from_slice(&[0; 4]); // CRC isn't checked for ancillary chunks
    let mut with_text = original[..33].to_vec();
    with_text.extend_from_slice(&chunk);
    with_text.extend_from_slice(&original[33..]);

    let out = embed(&with_text, &payload(), PASS, &EmbedOptions::default()).unwrap();
    assert!(out.data.windows(chunk.len()).any(|w| w == chunk));
}

#[test]
fn bmp_tiff_webp() {
    let rgb =
        || DynamicImage::ImageRgb8(ImageBuffer::from_raw(256, 200, photo(256, 200, 3, 4)).unwrap());
    roundtrip(&encode_image(rgb(), ImageFormat::Bmp), CarrierFormat::Bmp);
    roundtrip(&encode_image(rgb(), ImageFormat::Tiff), CarrierFormat::Tiff);
    roundtrip(&encode_image(rgb(), ImageFormat::WebP), CarrierFormat::WebP);
}

#[test]
fn jpeg() {
    let img =
        DynamicImage::ImageRgb8(ImageBuffer::from_raw(640, 480, photo(640, 480, 3, 5)).unwrap());
    let mut cover = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cover, 85)
        .encode_image(&img)
        .unwrap();
    // Add an EXIF-like APP1 segment after SOI to check markers are preserved.
    let app1 = b"\xFF\xE1\x00\x10Exif\0\0testdata";
    let mut with_exif = cover[..2].to_vec();
    with_exif.extend_from_slice(app1);
    with_exif.extend_from_slice(&cover[2..]);

    let info = inspect(&with_exif).unwrap();
    assert_eq!(info.format, CarrierFormat::Jpeg);
    assert!(info.capacity > payload_size(&payload()));

    let out = roundtrip(&with_exif, CarrierFormat::Jpeg);
    assert!(out.data.windows(app1.len()).any(|w| w == app1));
    // Still decodes as a normal image of the same size.
    let decoded = image::load_from_memory(&out.data).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (640, 480));
}

fn tone(frames: usize, channels: usize, scale: f64, seed: u64) -> Vec<f64> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..frames * channels)
        .map(|i| {
            let t = (i / channels) as f64 / 44_100.0;
            scale
                * (0.5 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                    + rng.random_range(-0.05..0.05))
        })
        .collect()
}

fn wav(spec: hound::WavSpec, samples: &[f64]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut out, spec).unwrap();
    for &s in samples {
        match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Float, _) => w.write_sample(s as f32).unwrap(),
            (_, 16) => w.write_sample(s as i16).unwrap(),
            _ => w.write_sample(s as i32).unwrap(),
        }
    }
    w.finalize().unwrap();
    out.into_inner()
}

fn spec(bits: u16, float: bool) -> hound::WavSpec {
    hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: bits,
        sample_format: if float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    }
}

#[test]
fn wav_formats() {
    roundtrip(
        &wav(spec(16, false), &tone(60_000, 2, 30_000.0, 6)),
        CarrierFormat::Wav,
    );
    roundtrip(
        &wav(spec(24, false), &tone(60_000, 2, 8_000_000.0, 7)),
        CarrierFormat::Wav,
    );
    roundtrip(
        &wav(spec(32, true), &tone(60_000, 2, 0.9, 8)),
        CarrierFormat::Wav,
    );
}

#[test]
fn wav_padded_from_16_bit_is_refused() {
    let padded: Vec<f64> = tone(60_000, 2, 30_000.0, 9)
        .iter()
        .map(|s| s.round() * 256.0)
        .collect();
    let data = wav(spec(24, false), &padded);
    assert!(matches!(inspect(&data), Err(Error::UnsuitableCarrier(_))));
}

#[test]
fn flac() {
    let samples: Vec<i32> = tone(60_000, 2, 30_000.0, 10)
        .iter()
        .map(|&s| s as i32)
        .collect();
    let config = flacenc::config::Encoder::default().into_verified().unwrap();
    let source = flacenc::source::MemSource::from_samples(&samples, 2, 16, 44_100);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).unwrap();
    let mut sink = flacenc::bitsink::ByteSink::new();
    flacenc::component::BitRepr::write(&stream, &mut sink).unwrap();
    roundtrip(sink.as_slice(), CarrierFormat::Flac);
}

#[test]
fn clean_carrier_reports_not_found() {
    assert!(matches!(extract(&rgb_png(), PASS), Err(Error::NotFound)));
}

#[test]
fn rejects_oversized_payload() {
    let carrier = rgb_png();
    let info = inspect(&carrier).unwrap();
    let mut rng = StdRng::seed_from_u64(11);
    let big: Vec<u8> = (0..info.capacity + 1).map(|_| rng.random()).collect();
    let entries = [Entry::file("big.bin", big, None)];
    assert!(matches!(
        embed(&carrier, &entries, PASS, &EmbedOptions::default()),
        Err(Error::PayloadTooLarge { .. })
    ));
}

#[test]
fn tampering_is_detected() {
    let out = embed(&rgb_png(), &payload(), PASS, &EmbedOptions::default()).unwrap();
    let mut img = image::load_from_memory(&out.data).unwrap().to_rgb8();
    for p in img.pixels_mut().step_by(3) {
        p[0] ^= 1;
    }
    let tampered = encode_image(DynamicImage::ImageRgb8(img), ImageFormat::Png);
    assert!(extract(&tampered, PASS).is_err());
}

#[test]
fn estimate_matches_exact_size() {
    let entries = payload();
    let estimate = estimate_payload_size(entries.iter().map(|e| (e.name.len(), e.data.len())));
    assert_eq!(estimate, payload_size(&entries));
}

#[test]
fn preset_cancel_stops_embed_and_extract() {
    let flag = AtomicBool::new(true);
    let ctl = Control {
        cancel: Some(&flag),
        progress: None,
    };
    let carrier = rgb_png();
    assert!(matches!(
        embed_with(&carrier, &payload(), PASS, &EmbedOptions::default(), &ctl),
        Err(Error::Cancelled)
    ));
    flag.store(false, Ordering::Relaxed);
    let hidden = embed(&carrier, &payload(), PASS, &EmbedOptions::default()).unwrap();
    flag.store(true, Ordering::Relaxed);
    assert!(matches!(
        extract_with(&hidden.data, PASS, &ctl),
        Err(Error::Cancelled)
    ));
}

#[test]
fn cancel_during_embedding() {
    let flag = AtomicBool::new(false);
    let progress = |stage: Stage, frac: f32| {
        if stage == Stage::Embedding && frac >= 0.3 {
            flag.store(true, Ordering::Relaxed);
        }
    };
    let ctl = Control {
        cancel: Some(&flag),
        progress: Some(&progress),
    };
    assert!(matches!(
        embed_with(&rgb_png(), &payload(), PASS, &EmbedOptions::default(), &ctl),
        Err(Error::Cancelled)
    ));
}

#[test]
fn progress_fractions_are_ordered() {
    let log = Mutex::new(Vec::new());
    let progress = |stage: Stage, frac: f32| log.lock().unwrap().push((stage, frac));
    let ctl = Control {
        cancel: None,
        progress: Some(&progress),
    };
    embed_with(&rgb_png(), &payload(), PASS, &EmbedOptions::default(), &ctl).unwrap();
    let events = log.lock().unwrap().clone();
    let mut last: Vec<(Stage, f32)> = Vec::new();
    let mut embedding_done = false;
    for (stage, frac) in events {
        assert!((0.0..=1.0).contains(&frac), "{stage:?} {frac}");
        if let Some((_, prev)) = last.iter().rev().find(|(s, _)| *s == stage) {
            assert!(*prev <= frac, "{stage:?} went {prev} -> {frac}");
        }
        if stage == Stage::Embedding && frac == 1.0 {
            embedding_done = true;
        }
        last.push((stage, frac));
    }
    assert!(embedding_done);
}

#[test]
fn resize_keeps_format_and_shrinks_capacity() {
    let rgb =
        DynamicImage::ImageRgb8(ImageBuffer::from_raw(160, 160, photo(160, 160, 3, 12)).unwrap());
    let mut rgba_px = photo(160, 160, 4, 13);
    for alpha in rgba_px.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    let rgba = DynamicImage::ImageRgba8(ImageBuffer::from_raw(160, 160, rgba_px).unwrap());
    let grey16: Vec<u16> = photo(160, 160, 1, 14)
        .iter()
        .map(|&v| v as u16 * 257)
        .collect();
    let grey = DynamicImage::ImageLuma16(ImageBuffer::from_raw(160, 160, grey16).unwrap());

    for (bytes, format) in [
        (
            encode_image(rgb.clone(), ImageFormat::Png),
            CarrierFormat::Png,
        ),
        (encode_image(rgba, ImageFormat::Png), CarrierFormat::Png),
        (encode_image(grey, ImageFormat::Png), CarrierFormat::Png),
        (
            encode_image(rgb.clone(), ImageFormat::Bmp),
            CarrierFormat::Bmp,
        ),
        (
            encode_image(rgb.clone(), ImageFormat::Tiff),
            CarrierFormat::Tiff,
        ),
        (encode_image(rgb, ImageFormat::WebP), CarrierFormat::WebP),
    ] {
        let before = inspect(&bytes).unwrap();
        let (w, h) = before.dimensions.expect("image size");
        assert!(before.sample_rate.is_none());
        let out = resize_image(&bytes, 0.5, &Control::none()).unwrap();
        assert_eq!(CarrierFormat::detect(&out).unwrap(), format);
        let after = inspect(&out).unwrap();
        assert_eq!(after.format, format);
        assert_eq!(
            after.dimensions,
            Some((
                (w as f32 * 0.5).round() as u32,
                (h as f32 * 0.5).round() as u32
            ))
        );
        assert!(after.capacity < before.capacity);
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) && before.description.contains("16-bit") {
            assert!(after.description.contains("16-bit"));
        }
    }
}

#[test]
fn resize_applies_jpeg_orientation() {
    let img = DynamicImage::ImageRgb8(ImageBuffer::from_raw(64, 32, photo(64, 32, 3, 15)).unwrap());
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&img)
        .unwrap();
    let oriented = insert_exif_orientation(&jpeg, 6);
    let out = resize_image(&oriented, 0.5, &Control::none()).unwrap();
    assert_eq!(CarrierFormat::detect(&out).unwrap(), CarrierFormat::Jpeg);
    let info = inspect(&out).unwrap();
    // 64×32 with orientation 6 becomes 32×64, then scale 0.5 → 16×32.
    assert_eq!(info.dimensions, Some((16, 32)));
    assert!(!out.windows(6).any(|w| w == b"Exif\0\0"));
}

#[test]
fn resize_scale_edges() {
    let png = rgb_png();
    assert_eq!(resize_image(&png, 1.0, &Control::none()).unwrap(), png);
    assert!(matches!(
        resize_image(&png, 0.0, &Control::none()),
        Err(Error::InvalidPayload(_))
    ));
    assert!(matches!(
        resize_image(&png, 1.5, &Control::none()),
        Err(Error::InvalidPayload(_))
    ));
    let audio = wav(spec(16, false), &tone(1000, 2, 1000.0, 16));
    assert!(matches!(
        resize_image(&audio, 0.5, &Control::none()),
        Err(Error::UnsupportedFormat(_))
    ));
}

#[test]
fn thumbnails_and_waveforms() {
    let rgb = DynamicImage::ImageRgb8(ImageBuffer::from_raw(80, 40, photo(80, 40, 3, 17)).unwrap());
    let rgba =
        DynamicImage::ImageRgba8(ImageBuffer::from_raw(40, 80, photo(40, 80, 4, 18)).unwrap());
    let grey =
        DynamicImage::ImageLuma8(ImageBuffer::from_raw(48, 48, photo(48, 48, 1, 19)).unwrap());
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&rgb)
        .unwrap();
    let cases = [
        (encode_image(rgb.clone(), ImageFormat::Png), "image/jpeg"),
        (encode_image(rgba, ImageFormat::Png), "image/png"),
        (encode_image(grey.clone(), ImageFormat::Png), "image/jpeg"),
        (encode_image(rgb.clone(), ImageFormat::Bmp), "image/jpeg"),
        (encode_image(rgb.clone(), ImageFormat::Tiff), "image/jpeg"),
        (encode_image(rgb, ImageFormat::WebP), "image/jpeg"),
        (jpeg, "image/jpeg"),
        (encode_image(grey, ImageFormat::Bmp), "image/jpeg"),
    ];
    for (bytes, mime) in cases {
        let (thumb, kind) = thumbnail(&bytes, 24).unwrap();
        assert_eq!(kind, mime);
        assert!(!thumb.is_empty());
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert!(decoded.width().max(decoded.height()) <= 24);
        assert!(matches!(
            waveform(&bytes, 16),
            Err(Error::UnsupportedFormat(_))
        ));
    }
    let audio = wav(spec(16, false), &tone(4000, 2, 20_000.0, 20));
    assert!(matches!(
        thumbnail(&audio, 24),
        Err(Error::UnsupportedFormat(_))
    ));
    let peaks = waveform(&audio, 16).unwrap();
    assert_eq!(peaks.len(), 16);
    assert!(peaks.iter().all(|v| (0.0..=1.0).contains(v)));
    assert!(peaks.iter().any(|v| *v > 0.0));
    assert!(matches!(waveform(&audio, 0), Err(Error::InvalidPayload(_))));
}

#[test]
fn ffmpeg_flac_roundtrip() {
    let stereo = include_bytes!("../tests/fixtures/ffmpeg-stereo.flac");
    let mono = include_bytes!("../tests/fixtures/ffmpeg-mono.flac");
    roundtrip(stereo, CarrierFormat::Flac);
    roundtrip(mono, CarrierFormat::Flac);
    let info = inspect(mono).unwrap();
    assert_eq!(info.channels, Some(1));
    assert_eq!(info.sample_rate, Some(44_100));
    assert!(info.frames.is_some_and(|n| n > 0));
    assert!(info.dimensions.is_none());
    let peaks = waveform(stereo, 32).unwrap();
    assert_eq!(peaks.len(), 32);
    assert!(peaks.iter().all(|v| (0.0..=1.0).contains(v)));
}

/// APP1 Exif segment with a single orientation tag, inserted after SOI.
fn insert_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&0u16.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&(payload.len() as u16 + 2).to_be_bytes());
    segment.extend_from_slice(&payload);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&segment);
    out.extend_from_slice(&jpeg[2..]);
    out
}
