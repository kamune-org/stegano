# Stegano

[![CI](https://github.com/kamune-org/stegano/actions/workflows/ci.yml/badge.svg)](https://github.com/kamune-org/stegano/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Stegano hides encrypted text messages and files inside images and audio. The payload is encrypted, then spread through the carrier so the result looks like an ordinary file of the same format.

Stegano is a cross-platform app built with [Tauri v2](https://v2.tauri.app/) (Rust) and [Vue 3](https://vuejs.org/). It runs on desktop and Android, and a command-line tool (`stegano`) is included for scripting and headless use.

Current version: **0.2.0**. Format v2 is not compatible with files made by 0.1.x.

## Features

- **Hide text and files:** embed an optional text message and any number of files in one carrier.
- **Strong encryption:** ChaCha20-Poly1305 with a passphrase-derived key (Argon2id).
- **Same format out:** the output is always the same format as the input file.
- **Metadata kept:** existing EXIF, ICC, XMP, and other metadata in the carrier is preserved where the format allows.
- **Capacity shown:** the app reports a maximum capacity and a recommended size for each carrier.
- **Desktop and Android:** one codebase for macOS, Windows, Linux, and Android, with a Vue UI.

## Supported carriers

| Format | Notes |
| --- | --- |
| PNG | 8/16-bit grey, grey+alpha, RGB, RGBA. Metadata chunks kept in place. |
| JPEG | Baseline and progressive. Data goes into DCT coefficients, so the output stays a JPEG with no recompression. APPn/COM markers (EXIF, ICC, XMP) kept. |
| BMP | Re-encoded without extra metadata. |
| TIFF | Re-encoded without extra metadata. |
| WebP | Lossless only. Re-encoded without extra metadata. |
| WAV | 16/24/32-bit integer and 32-bit float. Re-encoded without extra metadata. |
| FLAC | 16/24-bit. Tags and pictures kept; the seek table is dropped. |

Refused, with an explanation shown to the user:

- Palette PNGs and PNGs with a tRNS colour-key.
- Lossy or animated WebP.
- 8-bit audio, and audio padded up from a lower bit depth.
- Carriers that are too small for the payload.

## Payload

- Any number of files plus an optional text message, in one carrier.
- File names and modification times are kept.
- Optional DEFLATE compression (on by default; `--no-compress` in the CLI).
- Revealed file names are sanitised: no paths and no `..`.

## How it works

1. **Pack and encrypt.** The payload is packed into a container, optionally compressed, and encrypted with ChaCha20-Poly1305. The key comes from Argon2id (64 MiB, 3 passes) over the passphrase and a fresh random 16-byte salt for each embed. HKDF-SHA256 splits that key into independent subkeys for the permutation, header, and body.
2. **Find cover elements.** The carrier is reduced to cover elements: pixel colour channels (fully transparent pixels are skipped, and alpha is never changed), JPEG AC coefficients (DC is untouched), or audio samples.
3. **Place the data.** The salt goes into elements chosen by a public permutation. Since the salt is random, it looks like noise. The encrypted header (body length) and the body go into elements chosen by a passphrase-keyed permutation. Nothing is plaintext, nothing sits at a fixed position, and there is no magic signature.
4. **Embed with minimal distortion.** Bits are embedded with syndrome-trellis codes (STC), which minimise a content-adaptive distortion: HILL for pixels, UERD for JPEG, and a residual-based model for audio (digital silence is never touched). Changes are ±1 ("LSB matching"), not LSB replacement, which avoids the classic chi-square, RS, and SPA detectors.
5. **Authenticate.** Any modification of the stego file is detected by the AEAD tag. A wrong passphrase and "nothing hidden" give the same error, so the app never confirms that a carrier holds data.
6. **Verify.** After embedding, the output is decoded again to check that every element survived the encoder.

On real photos, hiding about 100 KB in an 8-megapixel JPEG changed about 0.7% of the AC coefficients. In an 8 MP PNG, about 0.55% of colour samples changed. That is roughly 0.17 to 0.22 changes per hidden bit, compared with 0.5 for naive LSB.

As a sanity check, Sample Pair Analysis (a classic LSB detector) estimates a 51% flip rate in the top rows of an image made with 0.1.x. On a 0.2 stego image carrying the same 100 KB it returns exactly the same value as on the untouched original. SPA targets LSB replacement only; machine-learning detectors are the real threat to ±1 embedding, which is what the recommended size guards against.

## Capacity

Each carrier shows two numbers:

- **Maximum capacity:** the most data the carrier can hold.
- **Recommended size:** about 0.1 bits per cover element (per non-zero AC coefficient for JPEG). Below this, detection by modern steganalysis is hard. Above it, the app shows a warning.

Example, a 3840×2160 photo:

| Carrier | Maximum | Recommended |
| --- | --- | --- |
| JPEG | ~1.2 MiB | ~125 KiB |
| PNG | ~1.5 MiB | ~300 KiB |

## Limitations

- **No steganography is undetectable.** Stay under the recommended size, and use carriers you have the only copy of (your own fresh photos), not images downloaded from the web.
- **Recompression destroys the data.** Messengers, social networks, and email clients that "optimise" images re-encode media and remove the hidden data. Send the output as a file or document.
- **Memory use is high.** Large images need a lot of RAM, around 800 MB to 1 GB for an 8 MP image.

## Command-line tool

The CLI binary is `stegano`, from the `crates/stegano-cli` crate.

```sh
stegano info photo.jpg
stegano hide photo.jpg -o out.jpg -m "meet at noon" -f plan.pdf -f keys.txt   # prompts for the passphrase twice
stegano reveal out.jpg -d ./extracted                                       # prints the text, writes the files; --force to overwrite
stegano --help
```

The passphrase comes from `--passphrase <value>` or the `STEGANO_PASSPHRASE` environment variable if set; otherwise you are prompted for it (twice when hiding).

## Installation

Release builds are attached to the project's GitHub Releases page:

- **macOS:** universal `.dmg` and `.app`.
- **Windows:** MSI and NSIS installers.
- **Linux:** AppImage, `.deb`, and `.rpm`.
- **Android:** APK and AAB.
- **CLI:** archives for Linux, macOS, and Windows.

Desktop builds are unsigned unless the release was made with signing secrets configured. Android builds without a release keystore are signed with a debug key: fine for sideloading, not for the Play Store.

## Building from source

Prerequisites:

- Rust 1.95 or newer.
- [Bun](https://bun.sh/).
- A C compiler. libjpeg is built from source via `mozjpeg-sys`; nasm is not required.
- Tauri's platform prerequisites: see the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).

Commands:

```sh
bun install
bun tauri dev        # run the desktop app in development
bun tauri build      # build a release bundle

cargo test -p stegano-core --release
cargo run -p stegano-cli -- --help
```

Android builds need the Android SDK and NDK, plus JDK 17:

```sh
bun tauri android build --apk
```

## Project layout

| Path | Contents |
| --- | --- |
| `crates/stegano-core` | All steganography and cryptography, with tests. |
| `crates/stegano-cli` | The `stegano` command-line tool. |
| `src-tauri` | Thin Tauri command layer. File I/O is in Rust, and Android content URIs are supported. |
| `src` | Vue 3 user interface. |
| `.github/workflows` | CI and release workflows. |

## CI and releases

- **`ci.yml`** runs on pushes to `main` and on pull requests. It checks rustfmt, runs clippy with `-D warnings`, runs the core tests, builds the CLI, and type-checks and builds the frontend.
- **`release.yml`** runs on `v*` tags or manually. It builds all platform bundles and CLI archives and attaches them to a **draft** GitHub release, which is published by hand. The tag must match the version in `src-tauri/tauri.conf.json`.
- **Signing** is optional and set with repository secrets. macOS uses `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, and `APPLE_SIGNING_IDENTITY`, plus `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID` for notarization. Android uses `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD`. Without them, desktop builds are unsigned and Android builds are debug-signed.
- **Dependabot** keeps GitHub Actions, Cargo, and Bun dependencies up to date.

## Background

The design draws on these papers:

- Filler, Judas, and Fridrich, "Minimizing additive distortion in steganography using syndrome-trellis codes" (2011).
- Li et al., "A new cost function for spatial image steganography" (HILL, 2014).
- Guo et al., "Using statistical image model for JPEG steganography: uniform embedding revisited" (UERD, 2015).

## License

Stegano is released under the [MIT License](LICENSE).
