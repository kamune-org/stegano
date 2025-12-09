use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::Argon2;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use hound::{WavReader, WavWriter};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use rand::RngCore;
use std::io::Cursor;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SteganoError {
    #[error("Image error: {0}")]
    ImageError(#[from] image::ImageError),
    #[error("Audio error: {0}")]
    AudioError(String),
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    #[error("Message too large for carrier")]
    MessageTooLarge,
    #[error("No hidden message found")]
    NoMessageFound,
    #[error("Invalid message format")]
    InvalidFormat,
    #[error("Decryption failed - wrong passphrase or corrupted data")]
    DecryptionFailed,
    #[error("Unsupported audio format")]
    UnsupportedAudioFormat,
}

impl serde::Serialize for SteganoError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

const MAGIC_HEADER: &[u8] = b"STEG";
const SALT_SIZE: usize = 16;
const NONCE_SIZE: usize = 12;

/// Derive a 256-bit key from a passphrase using Argon2
fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], SteganoError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| SteganoError::EncryptionError(e.to_string()))?;
    Ok(key)
}

/// Encrypt a message using AES-256-GCM
fn encrypt_message(message: &str, passphrase: &str) -> Result<Vec<u8>, SteganoError> {
    let mut salt = [0u8; SALT_SIZE];
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut nonce_bytes);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| SteganoError::EncryptionError(e.to_string()))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, message.as_bytes())
        .map_err(|e| SteganoError::EncryptionError(e.to_string()))?;

    // Format: MAGIC_HEADER + salt + nonce + ciphertext
    let mut result = Vec::new();
    result.extend_from_slice(MAGIC_HEADER);
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt a message using AES-256-GCM
fn decrypt_message(data: &[u8], passphrase: &str) -> Result<String, SteganoError> {
    // Check magic header
    if data.len() < MAGIC_HEADER.len() + SALT_SIZE + NONCE_SIZE {
        return Err(SteganoError::InvalidFormat);
    }

    if &data[..MAGIC_HEADER.len()] != MAGIC_HEADER {
        return Err(SteganoError::NoMessageFound);
    }

    let salt = &data[MAGIC_HEADER.len()..MAGIC_HEADER.len() + SALT_SIZE];
    let nonce_bytes =
        &data[MAGIC_HEADER.len() + SALT_SIZE..MAGIC_HEADER.len() + SALT_SIZE + NONCE_SIZE];
    let ciphertext = &data[MAGIC_HEADER.len() + SALT_SIZE + NONCE_SIZE..];

    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| SteganoError::EncryptionError(e.to_string()))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| SteganoError::DecryptionFailed)?;

    String::from_utf8(plaintext).map_err(|_| SteganoError::InvalidFormat)
}

// ============================================================================
// IMAGE STEGANOGRAPHY
// ============================================================================

/// Embed data into an image using LSB steganography
fn embed_data_image(img: &DynamicImage, data: &[u8]) -> Result<RgbaImage, SteganoError> {
    let (width, height) = img.dimensions();
    let max_bytes = ((width * height * 3) / 8) as usize - 4; // Reserve 4 bytes for length

    if data.len() > max_bytes {
        return Err(SteganoError::MessageTooLarge);
    }

    let mut output = img.to_rgba8();
    let data_len = data.len() as u32;

    // Create bit stream: 4 bytes for length + actual data
    let mut bit_stream = Vec::new();
    for byte in data_len.to_be_bytes() {
        for i in (0..8).rev() {
            bit_stream.push((byte >> i) & 1);
        }
    }
    for byte in data {
        for i in (0..8).rev() {
            bit_stream.push((byte >> i) & 1);
        }
    }

    let mut bit_index = 0;
    'outer: for y in 0..height {
        for x in 0..width {
            if bit_index >= bit_stream.len() {
                break 'outer;
            }

            let pixel = output.get_pixel_mut(x, y);
            let Rgba([r, g, b, a]) = *pixel;

            let new_r = if bit_index < bit_stream.len() {
                let bit = bit_stream[bit_index];
                bit_index += 1;
                (r & 0xFE) | bit
            } else {
                r
            };

            let new_g = if bit_index < bit_stream.len() {
                let bit = bit_stream[bit_index];
                bit_index += 1;
                (g & 0xFE) | bit
            } else {
                g
            };

            let new_b = if bit_index < bit_stream.len() {
                let bit = bit_stream[bit_index];
                bit_index += 1;
                (b & 0xFE) | bit
            } else {
                b
            };

            *pixel = Rgba([new_r, new_g, new_b, a]);
        }
    }

    Ok(output)
}

/// Extract data from an image using LSB steganography
fn extract_data_image(img: &DynamicImage) -> Result<Vec<u8>, SteganoError> {
    let (width, height) = img.dimensions();
    let rgba = img.to_rgba8();

    // First, extract the length (4 bytes = 32 bits)
    let mut bits = Vec::new();
    'outer: for y in 0..height {
        for x in 0..width {
            let Rgba([r, g, b, _]) = rgba.get_pixel(x, y);

            bits.push(r & 1);
            bits.push(g & 1);
            bits.push(b & 1);

            if bits.len() >= 32 {
                break 'outer;
            }
        }
    }

    if bits.len() < 32 {
        return Err(SteganoError::NoMessageFound);
    }

    // Convert first 32 bits to length
    let mut length_bytes = [0u8; 4];
    for (i, byte) in length_bytes.iter_mut().enumerate() {
        for j in 0..8 {
            *byte |= bits[i * 8 + j] << (7 - j);
        }
    }
    let data_length = u32::from_be_bytes(length_bytes) as usize;

    // Sanity check
    let max_bytes = ((width * height * 3) / 8) as usize - 4;
    if data_length > max_bytes || data_length == 0 {
        return Err(SteganoError::NoMessageFound);
    }

    // Extract the actual data
    let total_bits_needed = 32 + data_length * 8;
    let total_bits_available = (width * height * 3) as usize;
    if total_bits_needed > total_bits_available {
        return Err(SteganoError::NoMessageFound);
    }
    let mut all_bits = Vec::with_capacity(total_bits_needed);

    'outer2: for y in 0..height {
        for x in 0..width {
            let Rgba([r, g, b, _]) = rgba.get_pixel(x, y);

            all_bits.push(r & 1);
            if all_bits.len() >= total_bits_needed {
                break 'outer2;
            }

            all_bits.push(g & 1);
            if all_bits.len() >= total_bits_needed {
                break 'outer2;
            }

            all_bits.push(b & 1);
            if all_bits.len() >= total_bits_needed {
                break 'outer2;
            }
        }
    }

    // Convert bits to bytes (skip the first 32 bits which are the length)
    let mut data = Vec::with_capacity(data_length);
    for i in 0..data_length {
        let mut byte = 0u8;
        for j in 0..8 {
            byte |= all_bits[32 + i * 8 + j] << (7 - j);
        }
        data.push(byte);
    }

    Ok(data)
}

// ============================================================================
// AUDIO STEGANOGRAPHY
// ============================================================================

/// Embed data into audio samples using LSB steganography
fn embed_data_audio(audio_data: &[u8], data: &[u8]) -> Result<Vec<u8>, SteganoError> {
    let cursor = Cursor::new(audio_data);
    let reader = WavReader::new(cursor).map_err(|e| SteganoError::AudioError(e.to_string()))?;

    let spec = reader.spec();
    let samples: Vec<i32> = reader
        .into_samples::<i32>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SteganoError::AudioError(e.to_string()))?;

    // Calculate capacity (1 bit per sample, minus 32 bits for length)
    let max_bytes = (samples.len() / 8) - 4;
    if data.len() > max_bytes {
        return Err(SteganoError::MessageTooLarge);
    }

    let data_len = data.len() as u32;

    // Create bit stream: 4 bytes for length + actual data
    let mut bit_stream = Vec::new();
    for byte in data_len.to_be_bytes() {
        for i in (0..8).rev() {
            bit_stream.push((byte >> i) & 1);
        }
    }
    for byte in data {
        for i in (0..8).rev() {
            bit_stream.push((byte >> i) & 1);
        }
    }

    // Embed bits into samples
    let mut modified_samples = samples.clone();
    for (i, bit) in bit_stream.iter().enumerate() {
        if i < modified_samples.len() {
            // Clear LSB and set new bit
            modified_samples[i] = (modified_samples[i] & !1) | (*bit as i32);
        }
    }

    // Write output WAV
    let mut output_buffer = Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut output_buffer, spec)
            .map_err(|e| SteganoError::AudioError(e.to_string()))?;

        for sample in modified_samples {
            match spec.bits_per_sample {
                8 => writer
                    .write_sample(sample as i8)
                    .map_err(|e| SteganoError::AudioError(e.to_string()))?,
                16 => writer
                    .write_sample(sample as i16)
                    .map_err(|e| SteganoError::AudioError(e.to_string()))?,
                24 | 32 => writer
                    .write_sample(sample)
                    .map_err(|e| SteganoError::AudioError(e.to_string()))?,
                _ => return Err(SteganoError::UnsupportedAudioFormat),
            }
        }

        writer
            .finalize()
            .map_err(|e| SteganoError::AudioError(e.to_string()))?;
    }

    Ok(output_buffer.into_inner())
}

/// Extract data from audio samples using LSB steganography
fn extract_data_audio(audio_data: &[u8]) -> Result<Vec<u8>, SteganoError> {
    let cursor = Cursor::new(audio_data);
    let reader = WavReader::new(cursor).map_err(|e| SteganoError::AudioError(e.to_string()))?;

    let samples: Vec<i32> = reader
        .into_samples::<i32>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SteganoError::AudioError(e.to_string()))?;

    if samples.len() < 32 {
        return Err(SteganoError::NoMessageFound);
    }

    // Extract length (first 32 bits)
    let mut length_bits = Vec::new();
    for sample in samples.iter().take(32) {
        length_bits.push((sample & 1) as u8);
    }

    let mut length_bytes = [0u8; 4];
    for (i, byte) in length_bytes.iter_mut().enumerate() {
        for j in 0..8 {
            *byte |= length_bits[i * 8 + j] << (7 - j);
        }
    }
    let data_length = u32::from_be_bytes(length_bytes) as usize;

    // Sanity check
    let max_bytes = (samples.len() / 8) - 4;
    if data_length > max_bytes || data_length == 0 {
        return Err(SteganoError::NoMessageFound);
    }

    // Extract data bits
    let total_bits_needed = 32 + data_length * 8;
    if samples.len() < total_bits_needed {
        return Err(SteganoError::NoMessageFound);
    }

    let mut data = Vec::with_capacity(data_length);
    for i in 0..data_length {
        let mut byte = 0u8;
        for j in 0..8 {
            let sample_index = 32 + i * 8 + j;
            byte |= ((samples[sample_index] & 1) as u8) << (7 - j);
        }
        data.push(byte);
    }

    Ok(data)
}

// ============================================================================
// TAURI COMMANDS - IMAGE
// ============================================================================

#[tauri::command]
async fn encode_message(
    image_base64: String,
    message: String,
    passphrase: String,
) -> Result<String, SteganoError> {
    // Decode the base64 image
    let image_data = BASE64
        .decode(&image_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    // Load the image
    let img = image::load_from_memory(&image_data)?;

    // Encrypt the message
    let encrypted_data = encrypt_message(&message, &passphrase)?;

    // Embed the encrypted data into the image
    let output_img = embed_data_image(&img, &encrypted_data)?;

    // Encode the output image as PNG (lossless format is required for steganography)
    let mut output_buffer = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(output_img).write_to(&mut output_buffer, ImageFormat::Png)?;

    // Return as base64
    Ok(BASE64.encode(output_buffer.into_inner()))
}

#[tauri::command]
async fn decode_message(image_base64: String, passphrase: String) -> Result<String, SteganoError> {
    // Decode the base64 image
    let image_data = BASE64
        .decode(&image_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    // Load the image
    let img = image::load_from_memory(&image_data)?;

    // Extract the hidden data
    let encrypted_data = extract_data_image(&img)?;

    // Decrypt the message
    let message = decrypt_message(&encrypted_data, &passphrase)?;

    Ok(message)
}

#[tauri::command]
fn get_image_capacity(image_base64: String) -> Result<usize, SteganoError> {
    let image_data = BASE64
        .decode(&image_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    let img = image::load_from_memory(&image_data)?;
    let (width, height) = img.dimensions();

    // Calculate max bytes (subtract header overhead: magic + salt + nonce + auth tag)
    let overhead = MAGIC_HEADER.len() + SALT_SIZE + NONCE_SIZE + 16; // 16 is AES-GCM auth tag
    let raw_capacity = ((width * height * 3) / 8) as usize - 4;

    Ok(raw_capacity.saturating_sub(overhead))
}

// ============================================================================
// TAURI COMMANDS - AUDIO
// ============================================================================

#[tauri::command]
async fn encode_audio_message(
    audio_base64: String,
    message: String,
    passphrase: String,
) -> Result<String, SteganoError> {
    // Decode the base64 audio
    let audio_data = BASE64
        .decode(&audio_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    // Encrypt the message
    let encrypted_data = encrypt_message(&message, &passphrase)?;

    // Embed the encrypted data into the audio
    let output_audio = embed_data_audio(&audio_data, &encrypted_data)?;

    // Return as base64
    Ok(BASE64.encode(output_audio))
}

#[tauri::command]
async fn decode_audio_message(
    audio_base64: String,
    passphrase: String,
) -> Result<String, SteganoError> {
    // Decode the base64 audio
    let audio_data = BASE64
        .decode(&audio_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    // Extract the hidden data
    let encrypted_data = extract_data_audio(&audio_data)?;

    // Decrypt the message
    let message = decrypt_message(&encrypted_data, &passphrase)?;

    Ok(message)
}

#[tauri::command]
fn get_audio_capacity(audio_base64: String) -> Result<usize, SteganoError> {
    let audio_data = BASE64
        .decode(&audio_base64)
        .map_err(|_| SteganoError::InvalidFormat)?;

    let cursor = Cursor::new(audio_data);
    let reader = WavReader::new(cursor).map_err(|e| SteganoError::AudioError(e.to_string()))?;

    let num_samples = reader.len() as usize;

    // Calculate max bytes (subtract header overhead: magic + salt + nonce + auth tag)
    let overhead = MAGIC_HEADER.len() + SALT_SIZE + NONCE_SIZE + 16; // 16 is AES-GCM auth tag
    let raw_capacity = (num_samples / 8) - 4;

    Ok(raw_capacity.saturating_sub(overhead))
}

// ============================================================================
// TAURI APP ENTRY
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            encode_message,
            decode_message,
            get_image_capacity,
            encode_audio_message,
            decode_audio_message,
            get_audio_capacity
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
