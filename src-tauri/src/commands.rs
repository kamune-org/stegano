//! IPC commands. Names are snake_case here and camelCase in JavaScript.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use base64::Engine;
use serde::{Deserialize, Serialize};
use stegano_core::{CarrierFormat, CarrierKind, Entry, EntryKind};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
use zeroize::Zeroize;

use crate::error::{CmdResult, CommandError};
use crate::progress::Sink;
use crate::state::{AppState, Held, Import};

const CHUNK_LIMIT: usize = 2 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 1024 * 1024 * 1024;

#[cfg(target_os = "android")]
pub struct Picker<R: tauri::Runtime> {
    pub handle: tauri::plugin::PluginHandle<R>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CarrierDto {
    name: String,
    size: u64,
    format: &'static str,
    extension: &'static str,
    kind: &'static str,
    description: String,
    capacity: usize,
    recommended: usize,
    width: Option<u32>,
    height: Option<u32>,
    duration_ms: Option<u64>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    resizable: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileStat {
    path: String,
    name: String,
    size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HideRequest {
    carrier: FilePath,
    text: Option<String>,
    files: Vec<FilePath>,
    passphrase: String,
    compress: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HideReport {
    embedded_bytes: usize,
    changed: usize,
    elements: usize,
    capacity: usize,
    recommended: usize,
    output_size: usize,
    name: String,
    extension: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RevealedEntryDto {
    index: usize,
    kind: &'static str,
    name: String,
    size: usize,
    modified: Option<u64>,
    text: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Prepared {
    path: String,
    info: CarrierDto,
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PickFilesArgs {
    mime_types: Vec<String>,
    multiple: bool,
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PickSaveArgs {
    name: String,
    mime_type: String,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct PickFilesBody {
    files: Vec<FileStat>,
}

/// Format, description and capacity of a potential carrier.
#[tauri::command]
pub(crate) async fn inspect_carrier(app: AppHandle, path: FilePath) -> CmdResult<CarrierDto> {
    blocking(move || {
        let (data, _) = read_all(&app, &path)?;
        let info = stegano_core::inspect(&data)?;
        Ok(to_dto(&path, data.len() as u64, info))
    })
    .await
}

/// JPEG or PNG thumbnail, or 256 little-endian `f32` audio peaks.
#[tauri::command]
pub(crate) async fn carrier_preview(
    app: AppHandle,
    path: FilePath,
    max_size: u32,
) -> CmdResult<Response> {
    blocking(move || {
        let (data, _) = read_all(&app, &path)?;
        let bytes = match CarrierFormat::detect(&data)? {
            CarrierFormat::Wav | CarrierFormat::Flac => {
                let peaks = stegano_core::waveform(&data, 256)?;
                let mut bytes = Vec::with_capacity(peaks.len() * 4);
                for peak in peaks {
                    bytes.extend_from_slice(&peak.to_le_bytes());
                }
                bytes
            }
            _ => stegano_core::thumbnail(&data, max_size)?.0,
        };
        Ok(Response::new(bytes))
    })
    .await
}

/// Names and sizes of files the user picked to hide.
#[tauri::command]
pub(crate) async fn stat_files(app: AppHandle, paths: Vec<FilePath>) -> CmdResult<Vec<FileStat>> {
    blocking(move || {
        paths
            .iter()
            .map(|path| {
                let size = open_read(&app, path)?
                    .metadata()
                    .map_err(|err| io_err(path, err))?
                    .len();
                Ok(FileStat {
                    path: path.to_string(),
                    name: display_name(path),
                    size,
                })
            })
            .collect()
    })
    .await
}

/// Size of the payload container (before compression) for capacity checks.
#[tauri::command]
pub(crate) fn estimate_payload(text: Option<String>, files: Vec<FileStat>) -> usize {
    let text = text
        .filter(|item| !item.is_empty())
        .map(|item| (0, item.len()));
    stegano_core::estimate_payload_size(
        text.into_iter().chain(
            files
                .iter()
                .map(|file| (file.name.len(), file.size as usize)),
        ),
    )
}

/// Downscale an image into the app cache and return its new path.
#[tauri::command]
pub(crate) async fn prepare_carrier(
    app: AppHandle,
    state: State<'_, AppState>,
    path: FilePath,
    scale: f32,
    job_id: String,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<Prepared> {
    let guard = state.begin_job(&job_id);
    let flag = Arc::clone(&guard.flag);
    blocking(move || prepare_work(&app, &flag, path, scale, on_progress)).await
}

#[tauri::command]
pub(crate) async fn import_begin(app: AppHandle, name: String) -> CmdResult<String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let safe = sanitize_name(&name);
        let id = uuid::Uuid::new_v4().to_string();
        let path = work_dir(&app)?.join(format!("{id}-{safe}"));
        write_all(&app, &FilePath::Path(path.clone()), &[])?;
        state.insert_import(
            id.clone(),
            Import {
                path,
                name: safe,
                size: 0,
            },
        );
        Ok(id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_chunk(app: AppHandle, import_id: String, data: String) -> CmdResult<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .map_err(|_| CommandError::Other("invalid base64".into()))?;
        if bytes.len() > CHUNK_LIMIT {
            return Err(CommandError::Other(
                "import chunk is larger than 2 MiB".into(),
            ));
        }
        state.with_import_mut(&import_id, |import| {
            let next = import.size + bytes.len() as u64;
            if next > TOTAL_LIMIT {
                return Err(CommandError::Other("import is larger than 1 GiB".into()));
            }
            append_all(&app, &FilePath::Path(import.path.clone()), &bytes)?;
            import.size = next;
            Ok(())
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_finish(
    state: State<'_, AppState>,
    import_id: String,
) -> CmdResult<FileStat> {
    let import = state
        .take_import(&import_id)
        .ok_or_else(|| CommandError::Other("unknown import".into()))?;
    Ok(FileStat {
        path: import.path.display().to_string(),
        name: import.name,
        size: import.size,
    })
}

#[tauri::command]
pub(crate) async fn import_abort(state: State<'_, AppState>, import_id: String) -> CmdResult<()> {
    let Some(import) = state.take_import(&import_id) else {
        return Err(CommandError::Other("unknown import".into()));
    };
    blocking(move || {
        if import.path.exists() {
            std::fs::remove_file(&import.path)
                .map_err(|err| CommandError::Io(format!("{}: {err}", import.path.display())))?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub(crate) async fn hide(
    app: AppHandle,
    state: State<'_, AppState>,
    request: HideRequest,
    job_id: String,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<HideReport> {
    let guard = state.begin_job(&job_id);
    state.clear_held();
    let flag = Arc::clone(&guard.flag);
    let (bytes, report) = blocking(move || hide_work(&app, &flag, request, on_progress)).await?;
    state.set_held(Held {
        bytes,
        name: report.name.clone(),
        extension: report.extension.to_string(),
    });
    Ok(report)
}

#[tauri::command]
pub(crate) async fn save_output(
    app: AppHandle,
    state: State<'_, AppState>,
    path: FilePath,
) -> CmdResult<()> {
    let Some(bytes) = state.held_bytes() else {
        return Err(CommandError::Other("nothing to save".into()));
    };
    blocking(move || write_all(&app, &path, &bytes)).await
}

#[tauri::command]
pub(crate) fn discard_output(state: State<'_, AppState>) {
    state.clear_held();
}

#[tauri::command]
pub(crate) async fn reveal(
    app: AppHandle,
    state: State<'_, AppState>,
    carrier: FilePath,
    passphrase: String,
    job_id: String,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<Vec<RevealedEntryDto>> {
    let guard = state.begin_job(&job_id);
    state.clear_revealed();
    let flag = Arc::clone(&guard.flag);
    let entries =
        blocking(move || reveal_work(&app, &flag, carrier, passphrase, on_progress)).await?;
    let listing = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| RevealedEntryDto {
            index,
            kind: match entry.kind {
                EntryKind::Text => "text",
                EntryKind::File => "file",
            },
            name: match entry.kind {
                EntryKind::Text => String::new(),
                EntryKind::File => entry.safe_file_name(),
            },
            size: entry.data.len(),
            modified: entry.modified,
            text: (entry.kind == EntryKind::Text).then(|| entry.as_text()),
        })
        .collect();
    state.set_revealed(entries);
    Ok(listing)
}

/// Write a revealed entry (by index from the last `reveal`) to `path`.
#[tauri::command]
pub(crate) async fn save_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
    path: FilePath,
) -> CmdResult<()> {
    let Some(data) = state.revealed_bytes(index) else {
        return Err(CommandError::Other("nothing to save; reveal again".into()));
    };
    blocking(move || write_all(&app, &path, &data)).await
}

/// Forget revealed entries (wipes them from memory).
#[tauri::command]
pub(crate) fn clear_revealed(state: State<'_, AppState>) {
    state.clear_revealed();
}

/// Cancel a job. An unknown id is remembered briefly so a late start still stops.
#[tauri::command]
pub(crate) fn cancel_job(state: State<'_, AppState>, job_id: String) {
    state.jobs.cancel(&job_id);
}

#[tauri::command]
pub(crate) async fn pick_files(
    app: AppHandle,
    mime_types: Vec<String>,
    multiple: bool,
) -> CmdResult<Vec<FileStat>> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, mime_types, multiple);
        Err(CommandError::Other("only available on Android".into()))
    }
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<Picker<tauri::Wry>>().handle.clone();
        let body: PickFilesBody = handle
            .run_mobile_plugin_async(
                "pickFiles",
                PickFilesArgs {
                    mime_types,
                    multiple,
                },
            )
            .await
            .map_err(|err| CommandError::Other(err.to_string()))?;
        Ok(body.files)
    }
}

#[tauri::command]
pub(crate) async fn pick_save_path(
    app: AppHandle,
    name: String,
    mime_type: String,
) -> CmdResult<Option<String>> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, name, mime_type);
        Err(CommandError::Other("only available on Android".into()))
    }
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<Picker<tauri::Wry>>().handle.clone();
        handle
            .run_mobile_plugin_async("pickSavePath", PickSaveArgs { name, mime_type })
            .await
            .map_err(|err| CommandError::Other(err.to_string()))
    }
}

fn prepare_work(
    app: &AppHandle,
    flag: &AtomicBool,
    path: FilePath,
    scale: f32,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<Prepared> {
    let sink = Sink::new(on_progress);
    sink.emit("reading", 0.0);
    let (data, _) = read_all(app, &path)?;
    check_cancel(flag)?;
    sink.emit("reading", 0.05);
    let on_core = |stage, fraction| sink.resize(stage, fraction);
    let ctl = stegano_core::Control {
        cancel: Some(flag),
        progress: Some(&on_core),
    };
    let resized = stegano_core::resize_image(&data, scale, &ctl)?;
    sink.emit("writing", 0.80);
    let dest = work_dir(app)?.join(format!(
        "{}-{}-{}.{}",
        uuid::Uuid::new_v4(),
        file_stem(&display_name(&path)),
        (scale * 100.0).round() as i32,
        CarrierFormat::detect(&resized)?.extension()
    ));
    write_all(app, &FilePath::Path(dest.clone()), &resized)?;
    sink.emit("writing", 0.95);
    let info = stegano_core::inspect(&resized)?;
    let prepared = Prepared {
        path: dest.display().to_string(),
        info: to_dto(&FilePath::Path(dest), resized.len() as u64, info),
    };
    sink.emit("writing", 0.99);
    sink.finish("writing");
    Ok(prepared)
}

fn hide_work(
    app: &AppHandle,
    flag: &AtomicBool,
    request: HideRequest,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<(Vec<u8>, HideReport)> {
    let HideRequest {
        carrier,
        text,
        files,
        mut passphrase,
        compress,
    } = request;
    let mut entries = Vec::new();
    let outcome = (|| {
        if passphrase.is_empty() {
            return Err(CommandError::Other("a passphrase is required".into()));
        }
        let sink = Sink::new(on_progress);
        let steps = files.len() + 1;
        sink.emit("reading", 0.0);
        if let Some(text) = text.filter(|item| !item.is_empty()) {
            entries.push(Entry::text(text));
        }
        for (index, path) in files.iter().enumerate() {
            let (data, modified) = read_all(app, path)?;
            entries.push(Entry::file(display_name(path), data, modified));
            check_cancel(flag)?;
            sink.emit("reading", 0.05 * (index + 1) as f32 / steps as f32);
        }
        let (carrier_data, _) = read_all(app, &carrier)?;
        check_cancel(flag)?;
        sink.emit("reading", 0.05);
        let on_core = |stage, fraction| sink.hide(stage, fraction);
        let ctl = stegano_core::Control {
            cancel: Some(flag),
            progress: Some(&on_core),
        };
        let result = stegano_core::embed_with(
            &carrier_data,
            &entries,
            &passphrase,
            &stegano_core::EmbedOptions { compress },
            &ctl,
        )?;
        let extension = result.format.extension();
        let report = HideReport {
            embedded_bytes: result.report.embedded_bytes,
            changed: result.report.changed,
            elements: result.report.elements,
            capacity: result.report.capacity,
            recommended: result.report.recommended,
            output_size: result.data.len(),
            name: format!("{}.{}", file_stem(&display_name(&carrier)), extension),
            extension,
        };
        sink.finish("verifying");
        Ok((result.data, report))
    })();
    passphrase.zeroize();
    for entry in &mut entries {
        entry.data.zeroize();
    }
    outcome
}

fn reveal_work(
    app: &AppHandle,
    flag: &AtomicBool,
    carrier: FilePath,
    mut passphrase: String,
    on_progress: Channel<crate::progress::Progress>,
) -> CmdResult<Vec<Entry>> {
    let outcome = (|| {
        if passphrase.is_empty() {
            return Err(CommandError::Other("a passphrase is required".into()));
        }
        let sink = Sink::new(on_progress);
        sink.emit("reading", 0.0);
        let (data, _) = read_all(app, &carrier)?;
        check_cancel(flag)?;
        sink.emit("reading", 0.10);
        sink.emit("analyzing", 0.40);
        let on_core = |stage, fraction| sink.reveal(stage, fraction);
        let ctl = stegano_core::Control {
            cancel: Some(flag),
            progress: Some(&on_core),
        };
        let entries = stegano_core::extract_with(&data, &passphrase, &ctl)?;
        sink.finish("decrypting");
        Ok(entries)
    })();
    passphrase.zeroize();
    outcome
}

fn to_dto(path: &FilePath, size: u64, info: stegano_core::CarrierInfo) -> CarrierDto {
    let (width, height) = match info.dimensions {
        Some((width, height)) => (Some(width), Some(height)),
        None => (None, None),
    };
    let duration_ms = match (info.frames, info.sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => {
            Some(frames.saturating_mul(1000) / u64::from(rate))
        }
        _ => None,
    };
    CarrierDto {
        name: display_name(path),
        size,
        format: info.format.name(),
        extension: info.format.extension(),
        kind: match info.kind {
            CarrierKind::Image => "image",
            CarrierKind::Audio => "audio",
        },
        description: info.description,
        capacity: info.capacity,
        recommended: info.recommended,
        width,
        height,
        duration_ms,
        sample_rate: info.sample_rate,
        channels: info.channels,
        resizable: info.kind == CarrierKind::Image,
    }
}

fn work_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|err| CommandError::Other(err.to_string()))?
        .join("work");
    std::fs::create_dir_all(&dir).map_err(|err| CommandError::Io(err.to_string()))?;
    Ok(dir)
}

fn sanitize_name(name: &str) -> String {
    if name.contains('\0') {
        return "file".into();
    }
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if base.is_empty() || base == "." || base == ".." {
        "file".into()
    } else {
        base.to_string()
    }
}

fn file_stem(name: &str) -> String {
    let base = sanitize_name(name);
    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() && stem != "." && stem != ".." => stem.to_string(),
        _ => base,
    }
}

fn io_err(path: &FilePath, err: impl std::fmt::Display) -> CommandError {
    CommandError::Io(format!("{}: {err}", display_name(path)))
}

fn check_cancel(flag: &AtomicBool) -> CmdResult<()> {
    if flag.load(Ordering::Relaxed) {
        Err(stegano_core::Error::Cancelled.into())
    } else {
        Ok(())
    }
}

/// Name shown to the user. Files in the work dir drop a leading `<uuid>-`
/// (`<uuid>-photo-50.jpg` → `photo-50.jpg`, `<uuid>-notes.txt` → `notes.txt`).
/// The same name outside the work dir keeps the prefix.
fn visible_file_name(name: &str, in_work_dir: bool) -> &str {
    if !in_work_dir {
        return name;
    }
    let Some(rest) = name.get(37..) else {
        return name;
    };
    if rest.is_empty() || name.as_bytes()[36] != b'-' {
        return name;
    }
    if uuid::Uuid::parse_str(&name[..36]).is_ok() {
        rest
    } else {
        name
    }
}

fn display_name(path: &FilePath) -> String {
    match path {
        FilePath::Path(path) => {
            let in_work_dir = path
                .parent()
                .and_then(|parent| parent.file_name())
                .is_some_and(|dir| dir == "work");
            path.file_name()
                .map(|name| {
                    let name = name.to_string_lossy();
                    visible_file_name(&name, in_work_dir).to_string()
                })
                .unwrap_or_else(|| path.display().to_string())
        }
        FilePath::Url(url) => {
            let last = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .unwrap_or_default();
            percent_decode(last)
                .rsplit(['/', ':'])
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or("file")
                .to_string()
        }
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|chunk| std::str::from_utf8(chunk).ok())
            .and_then(|chunk| u8::from_str_radix(chunk, 16).ok());
        if let (b'%', Some(byte)) = (bytes[index], hex) {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_read(app: &AppHandle, path: &FilePath) -> CmdResult<std::fs::File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    app.fs()
        .open(path.clone(), opts)
        .map_err(|err| io_err(path, err))
}

fn read_all(app: &AppHandle, path: &FilePath) -> CmdResult<(Vec<u8>, Option<u64>)> {
    let mut file = open_read(app, path)?;
    let modified = file
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs());
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|err| io_err(path, err))?;
    Ok((data, modified))
}

fn write_all(app: &AppHandle, path: &FilePath, data: &[u8]) -> CmdResult<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut file = app
        .fs()
        .open(path.clone(), opts)
        .map_err(|err| io_err(path, err))?;
    file.write_all(data).map_err(|err| io_err(path, err))?;
    file.flush().map_err(|err| io_err(path, err))
}

fn append_all(app: &AppHandle, path: &FilePath, data: &[u8]) -> CmdResult<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).append(true);
    let mut file = app
        .fs()
        .open(path.clone(), opts)
        .map_err(|err| io_err(path, err))?;
    file.write_all(data).map_err(|err| io_err(path, err))?;
    file.flush().map_err(|err| io_err(path, err))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| CommandError::Other(err.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::visible_file_name;

    const PREPARED: &str = "3f2a1c4e-5b6d-4789-a012-3456789abcde-photo-50.jpg";
    const IMPORTED: &str = "3f2a1c4e-5b6d-4789-a012-3456789abcde-notes.txt";

    #[test]
    fn visible_file_name_drops_work_dir_uuid() {
        assert_eq!(visible_file_name(PREPARED, true), "photo-50.jpg");
        assert_eq!(visible_file_name(IMPORTED, true), "notes.txt");
        assert_eq!(visible_file_name(PREPARED, false), PREPARED);
        assert_eq!(visible_file_name("photo-50.jpg", true), "photo-50.jpg");
    }
}
