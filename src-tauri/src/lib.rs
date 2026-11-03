//! Tauri commands. All steganography lives in `stegano-core`; this layer only
//! moves bytes between the filesystem and the core.
//!
//! Files are read and written in Rust through the fs plugin, which also handles
//! Android `content://` URIs, so payloads never cross the IPC bridge as base64.
//! Revealed entries stay in Rust memory until saved or cleared.

use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use stegano_core::{CarrierKind, EmbedOptions, Entry, EntryKind};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
use zeroize::Zeroize;

#[derive(Debug, thiserror::Error)]
enum CommandError {
    #[error(transparent)]
    Core(#[from] stegano_core::Error),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

type CmdResult<T> = Result<T, CommandError>;

/// Entries from the last successful reveal.
#[derive(Default)]
struct Revealed(Mutex<Vec<Entry>>);

impl Revealed {
    fn replace(&self, entries: Vec<Entry>) {
        let mut guard = self.0.lock().unwrap();
        for e in guard.iter_mut() {
            e.data.zeroize();
        }
        *guard = entries;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CarrierInfo {
    name: String,
    size: u64,
    format: &'static str,
    extension: &'static str,
    kind: &'static str,
    description: String,
    capacity: usize,
    recommended: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileStat {
    path: String,
    name: String,
    size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HideRequest {
    carrier: FilePath,
    output: FilePath,
    text: Option<String>,
    files: Vec<FilePath>,
    passphrase: String,
    compress: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HideReport {
    embedded_bytes: usize,
    changed: usize,
    elements: usize,
    capacity: usize,
    recommended: usize,
    output_size: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevealedEntry {
    index: usize,
    kind: &'static str,
    name: String,
    size: usize,
    modified: Option<u64>,
    text: Option<String>,
}

fn io_err(path: &FilePath, e: impl std::fmt::Display) -> CommandError {
    CommandError::Io(format!("{}: {e}", display_name(path)))
}

/// Best-effort file name for a path or URI (including Android content URIs,
/// whose last segment is often an encoded `primary:Download/name.ext`).
fn display_name(path: &FilePath) -> String {
    match path {
        FilePath::Path(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string()),
        FilePath::Url(u) => {
            let last = u
                .path_segments()
                .and_then(|mut s| s.next_back())
                .unwrap_or_default();
            let decoded = percent_decode(last);
            decoded
                .rsplit(['/', ':'])
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("file")
                .to_string()
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(b)) = (bytes[i], hex) {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_read(app: &AppHandle, path: &FilePath) -> CmdResult<std::fs::File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    app.fs()
        .open(path.clone(), opts)
        .map_err(|e| io_err(path, e))
}

fn read_all(app: &AppHandle, path: &FilePath) -> CmdResult<(Vec<u8>, Option<u64>)> {
    let mut file = open_read(app, path)?;
    let modified = file
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| io_err(path, e))?;
    Ok((data, modified))
}

fn write_all(app: &AppHandle, path: &FilePath, data: &[u8]) -> CmdResult<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut file = app
        .fs()
        .open(path.clone(), opts)
        .map_err(|e| io_err(path, e))?;
    file.write_all(data).map_err(|e| io_err(path, e))?;
    file.flush().map_err(|e| io_err(path, e))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::Other(e.to_string()))?
}

/// Format, description and capacity of a potential carrier.
#[tauri::command]
async fn inspect_carrier(app: AppHandle, path: FilePath) -> CmdResult<CarrierInfo> {
    blocking(move || {
        let (data, _) = read_all(&app, &path)?;
        let info = stegano_core::inspect(&data)?;
        Ok(CarrierInfo {
            name: display_name(&path),
            size: data.len() as u64,
            format: info.format.name(),
            extension: info.format.extension(),
            kind: match info.kind {
                CarrierKind::Image => "image",
                CarrierKind::Audio => "audio",
            },
            description: info.description,
            capacity: info.capacity,
            recommended: info.recommended,
        })
    })
    .await
}

/// Names and sizes of files the user picked to hide.
#[tauri::command]
async fn stat_files(app: AppHandle, paths: Vec<FilePath>) -> CmdResult<Vec<FileStat>> {
    blocking(move || {
        paths
            .iter()
            .map(|p| {
                let size = open_read(&app, p)?
                    .metadata()
                    .map_err(|e| io_err(p, e))?
                    .len();
                Ok(FileStat {
                    path: p.to_string(),
                    name: display_name(p),
                    size,
                })
            })
            .collect()
    })
    .await
}

/// Size of the payload container (before compression) for capacity checks.
#[tauri::command]
fn estimate_payload(text: Option<String>, files: Vec<FileStat>) -> usize {
    let text = text.filter(|t| !t.is_empty()).map(|t| (0, t.len()));
    stegano_core::estimate_payload_size(
        text.into_iter()
            .chain(files.iter().map(|f| (f.name.len(), f.size as usize))),
    )
}

#[tauri::command]
async fn hide(app: AppHandle, request: HideRequest) -> CmdResult<HideReport> {
    blocking(move || {
        let HideRequest {
            carrier,
            output,
            text,
            files,
            mut passphrase,
            compress,
        } = request;
        if passphrase.is_empty() {
            return Err(CommandError::Other("a passphrase is required".into()));
        }
        let mut entries = Vec::new();
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            entries.push(Entry::text(text));
        }
        for path in &files {
            let (data, modified) = read_all(&app, path)?;
            entries.push(Entry::file(display_name(path), data, modified));
        }
        let (carrier_data, _) = read_all(&app, &carrier)?;
        let result = stegano_core::embed(
            &carrier_data,
            &entries,
            &passphrase,
            &EmbedOptions { compress },
        );
        passphrase.zeroize();
        for e in &mut entries {
            e.data.zeroize();
        }
        let result = result?;
        write_all(&app, &output, &result.data)?;
        let r = result.report;
        Ok(HideReport {
            embedded_bytes: r.embedded_bytes,
            changed: r.changed,
            elements: r.elements,
            capacity: r.capacity,
            recommended: r.recommended,
            output_size: result.data.len(),
        })
    })
    .await
}

#[tauri::command]
async fn reveal(
    app: AppHandle,
    state: State<'_, Revealed>,
    carrier: FilePath,
    mut passphrase: String,
) -> CmdResult<Vec<RevealedEntry>> {
    state.replace(Vec::new());
    let entries = blocking(move || {
        let (data, _) = read_all(&app, &carrier)?;
        let entries = stegano_core::extract(&data, &passphrase);
        passphrase.zeroize();
        Ok(entries?)
    })
    .await?;

    let listing = entries
        .iter()
        .enumerate()
        .map(|(index, e)| RevealedEntry {
            index,
            kind: match e.kind {
                EntryKind::Text => "text",
                EntryKind::File => "file",
            },
            name: match e.kind {
                EntryKind::Text => String::new(),
                EntryKind::File => e.safe_file_name(),
            },
            size: e.data.len(),
            modified: e.modified,
            text: (e.kind == EntryKind::Text).then(|| e.as_text()),
        })
        .collect();
    state.replace(entries);
    Ok(listing)
}

/// Write a revealed entry (by index from the last `reveal`) to `path`.
#[tauri::command]
async fn save_entry(
    app: AppHandle,
    state: State<'_, Revealed>,
    index: usize,
    path: FilePath,
) -> CmdResult<()> {
    let data = state
        .0
        .lock()
        .unwrap()
        .get(index)
        .map(|e| e.data.clone())
        .ok_or_else(|| CommandError::Other("nothing to save; reveal again".into()))?;
    blocking(move || write_all(&app, &path, &data)).await
}

/// Forget revealed entries (wipes them from memory).
#[tauri::command]
fn clear_revealed(state: State<'_, Revealed>) {
    state.replace(Vec::new());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Revealed::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            inspect_carrier,
            stat_files,
            estimate_payload,
            hide,
            reveal,
            save_entry,
            clear_revealed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
