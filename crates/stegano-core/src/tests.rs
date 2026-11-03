//! End-to-end tests over every carrier format, using synthetic carriers.

use std::io::Cursor;

use flacenc::error::Verify;

use image::{DynamicImage, ImageBuffer, ImageFormat};
use rand::{Rng, SeedableRng, rngs::StdRng};

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
                let noise = rng.gen_range(-12.0..12.0);
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
                    + rng.gen_range(-0.05..0.05))
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
    let big: Vec<u8> = (0..info.capacity + 1).map(|_| rng.r#gen()).collect();
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
