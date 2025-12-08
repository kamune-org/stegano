# Stegano

Stegano is a cross-platform desktop application for hiding and extracting secret messages in images using steganography and encryption. Built with [Tauri](https://tauri.app/) (Rust backend) and [Vue 3](https://vuejs.org/) (TypeScript frontend), it provides a simple and secure way to encode and decode messages in PNG images.

## Features

- **Hide messages in images:** Securely embed encrypted text messages inside PNG images using LSB steganography and AES-256-GCM encryption.
- **Extract hidden messages:** Decode and decrypt messages from images using your passphrase.
- **Passphrase protection:** Messages are encrypted with a passphrase for privacy.
- **Capacity check:** See how much data can be hidden in a selected image.
- **Modern UI:** Fast, responsive interface built with Vue 3.

## Usage

1. **Encode a message:**
   - Select a PNG image.
   - Enter your secret message and a passphrase.
   - Click "Encode" to embed the message.
   - Save the resulting image.

2. **Decode a message:**
   - Select an image with a hidden message.
   - Enter the passphrase used for encoding.
   - Click "Decode" to reveal the message.
