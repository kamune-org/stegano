# Stegano 0.3: redesign spec

This is the design for the 0.3 rework: a new UI, progress and cancel for long
operations, in-app camera and audio capture, image downscaling, and fixes for
the Android keyboard and file picker. Video carriers are out of scope for 0.3
(see the last section).

## 1. Goals

1. A UI that feels like a native app on a phone and on a desktop, with clear
   steps, instant feedback, and no dead ends.
2. Android: text fields keep the keyboard open; picked files always appear.
3. Every operation that can take more than ~300 ms shows a determinate
   progress bar with a stage label and a working Cancel button.
4. Take a photo or record audio inside the app and use it as the carrier.
5. Shrink an image carrier to 100 / 80 / 50 / 20 % before hiding.

## 2. Stack

| Layer | Choice | Why |
| --- | --- | --- |
| Framework | Vue 3.5 (`<script setup>`, TS), Vite | The problems were design and platform glue, not Vue. Keeps the team's knowledge. |
| Styling | Tailwind CSS v4 (`@tailwindcss/vite`), CSS variables for theme tokens | Fast, consistent spacing/typography; dark and light themes from tokens. |
| Primitives | Reka UI (headless, accessible: Dialog, Tabs, ToggleGroup, Collapsible, Progress, DropdownMenu, Tooltip) | Focus management, ARIA and keyboard handling for free; we own the look. |
| Icons | `@lucide/vue` | One consistent icon set. |
| Toasts | `vue-sonner` | Errors and notices that don't push layout around. |
| State | Pinia (`stores/hide.ts`, `stores/reveal.ts`) | State survives tab switches; views stay thin. |

No other runtime dependencies without a reason.

## 3. Information architecture

Two modes, **Hide** and **Reveal**.

The layout follows the input device, not only the width (`src/lib/layout.ts`
sets `data-ui="phone|desktop"` and `data-split` on `<html>`):

- **Phone (coarse pointer, and always on Android):** top app bar with the
  mode title, a "Cover · Secret · Passphrase" step header, a scrolling
  column of cards, a sticky **action bar** (capacity summary + primary
  button), and a **bottom navigation bar** with Hide / Reveal. While a text
  field is focused, the bottom nav and action bar are hidden (they come back
  on blur), so the keyboard area is all content.
- **Desktop (fine pointer):** native density at every width: the system UI
  font at 13 px, 28 px controls, arrow cursor, focus rings only for keyboard
  focus, non-selectable chrome. A unified 52 px toolbar is the window's
  title bar on macOS (overlay title bar, traffic lights on the left): Hide /
  Reveal segmented control in the centre, the primary action on the right
  with the "why disabled" hint beside it. At 860 px and wider the window is a
  **split view**: the cover file on the left like Preview (large preview,
  info bar, a Size drop-down), and an inspector-style form on the right that
  scrolls on its own. Narrower windows stack the two in one column. Keyboard
  shortcuts: ⌘O choose cover, ⇧⌘O add files, ⌘↩ Hide / Reveal, ⌘S save,
  ⌘1 / ⌘2 switch mode, Esc closes dialogs (Ctrl on Windows and Linux).
- No app logo inside the window: the Dock, taskbar and title bar already
  identify the app.
- Both views stay mounted (`v-show`), so switching tabs keeps the Hide form.
  Revealed content is wiped when the user leaves Reveal (as in 0.2), with a
  small note saying so.

## 4. Screens

### 4.1 Hide

1. **Carrier card**
   - Empty: a dashed drop zone ("Drop an image or audio file") with three
     large buttons: **Choose file**, **Take photo**, **Record audio**. Camera
     and microphone buttons only appear when
     `navigator.mediaDevices?.getUserMedia` exists.
   - Loading: as soon as a file is picked, show its name and a skeleton
     preview immediately, then fill in details when `inspect_carrier`
     returns. Never leave the user wondering whether the pick worked.
   - Loaded: preview (image thumbnail at its aspect ratio, or audio waveform
     with duration), file name, chips (format, W×H or duration and channels,
     file size), and "Holds up to X · Safe up to Y".
   - Images only: **Size** segmented control 100 % / 80 % / 50 % / 20 %. Each
     option shows the resulting W×H and an approximate capacity
     (`capacity × s²`, marked ≈). Picking one runs `prepare_carrier`
     (progress shown inline in the card, cancellable). The real capacity
     replaces the estimate when it finishes. A note says resizing re-encodes
     the image and removes metadata.
   - Actions: **Change** (menu: choose file / take photo / record audio) and
     **Remove**.
2. **Secret card:** auto-growing message textarea with a byte counter; a list
   of files (type icon, name, size, remove); **Add files**. On desktop the card
   is a drop target for files.
3. **Passphrase card:** passphrase and confirm fields with show/hide toggles,
   a strength meter, and **Generate**, which fills both fields with a random
   passphrase (5 groups of 4 characters from an unambiguous alphabet, about
   100 bits, from `crypto.getRandomValues`), reveals it, and offers Copy.
   Passphrase fields use `autocomplete="off"`, `autocapitalize="off"`,
   `spellcheck="false"`.
4. **Options** (collapsed): Compress payload (on by default).
5. **Action bar:** compact capacity meter (green / amber / red against the safe
   and maximum sizes) and **Hide**. When disabled, the bar says why ("Choose a
   carrier", "Add a message or a file", "Passphrases don't match", "Too large
   for this carrier").

Flow: **Hide** → progress sheet → result sheet. The output is kept in memory
until the user saves it, so a cancelled or failed run never leaves an empty
file behind.

- **Progress sheet** (bottom sheet on phones, centred dialog on desktop):
  stage label, determinate bar with percent, elapsed time, **Cancel**. After
  Cancel the button reads "Cancelling…" until the backend confirms. The sheet
  cannot be dismissed by tapping outside.
- **Result sheet:** success icon, "Hidden 12.4 KiB in photo.jpg", details
  (changed elements and %, output size), **Save…** (save dialog with the
  default name `<stem>.<ext>`; on desktop, warn if it is the carrier itself),
  then "Saved" with **Show in folder** on desktop, and **Done**, which clears
  the passphrases and the held output. The message and files stay so the user
  can hide them again elsewhere.

### 4.2 Reveal

1. Carrier card (choose file or drop only; no resize, no capture).
2. Passphrase field. Enter submits.
3. Action bar: **Reveal**.
4. Progress sheet as above.
5. Results: each text entry in a card with the message (selectable, preserves
   whitespace) and **Copy**; files in a list with name, size, date and
   **Save…**; **Clear** at the top. A wrong passphrase shows the error inline
   under the passphrase field, not only as a toast.

### 4.3 Camera capture (full-screen overlay)

- Live preview from `getUserMedia({ video: { facingMode: "environment",
  width: { ideal: 4096 }, height: { ideal: 3072 } }, audio: false })`.
- Shutter button, switch camera (when more than one video input), close.
- After capture: review frame with **Retake** and **Use photo**.
- Encode the frame to PNG (lossless, most capacity) with a canvas, send the
  bytes with `import_begin` / `import_chunk` / `import_finish` (progress shown),
  then inspect it like a picked file. Name:
  `photo-YYYYMMDD-HHMMSS.png`.
- Stop all tracks when the overlay closes, also on errors.

### 4.4 Audio recorder (sheet)

- `getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false,
  autoGainControl: false, channelCount: 1 } })`. The processing filters are
  off on purpose: natural noise is what hides the data.
- An AudioWorklet (`public/recorder-worklet.js`, plain JS so the CSP allows
  it) collects Float32 samples at the context's sample rate.
- UI: big record/stop button, timer, live level meter, max 10 minutes. After
  stopping: play back, **Re-record**, **Use recording**.
- Encode 16-bit PCM mono WAV in TS (`lib/wav.ts`, with dither-free rounding
  and clamping), send it the same way. Name: `recording-YYYYMMDD-HHMMSS.wav`.

### 4.5 Feedback rules

- Errors that belong to a field show next to it; everything else is a toast.
- Backend errors arrive as `{ code, message }` (section 6). `cancelled` is
  never shown as an error.
- Android back button closes the topmost sheet or overlay (push a history
  entry when one opens, close it on `popstate`).
- No layout shift around the focused field: nothing above a focused input is
  inserted or removed while it has focus.

## 5. Visual system

- Tokens as CSS variables on `:root` (light) and `.dark` / `prefers-color-scheme`
  (dark): background, surface, surface-raised, border, text, text-muted,
  primary (indigo, around #5B5BD6), success, warning, danger, focus ring.
- Type: Inter on phones (16 px base, no zoom on focus), the system UI font on
  desktop (13 px); JetBrains Mono for passphrases and revealed text. Both
  bundled, so they work offline.
- Phones: radius 16 px on cards, 12 px on controls, touch targets at least
  44 px. Desktop: no cards, 28 px controls with 6 px radius.
- Motion: 150–200 ms ease-out for sheets and toasts; respect
  `prefers-reduced-motion`.
- Safe areas: `env(safe-area-inset-*)` padding on app bar, bottom nav and
  action bar (desktop/iOS). On Android the native side pads the WebView (section 7).
- Accessibility: visible focus rings, labels on every control, `aria-live` for
  progress and capacity, contrast AA in both themes.

## 6. Backend contract (Tauri commands)

All paths are opaque strings (filesystem paths on desktop, `content://` URIs
on Android). Long operations take a `jobId` (UUID chosen by the frontend) and
a `tauri::ipc::Channel<Progress>`.

```ts
type CommandError = { code: "cancelled" | "not_found" | "too_large" | "unsuitable"
                     | "unsupported" | "corrupt" | "io" | "other"; message: string };

type Stage = "reading" | "analyzing" | "keying" | "embedding" | "encoding"
           | "verifying" | "extracting" | "decrypting" | "resizing" | "writing";
type Progress = { stage: Stage; fraction: number }; // overall 0..1, monotonic

type CarrierInfo = {
  name: string; size: number; format: string; extension: string;
  kind: "image" | "audio"; description: string;
  capacity: number; recommended: number;
  width: number | null; height: number | null;            // images
  durationMs: number | null; sampleRate: number | null;   // audio
  channels: number | null;
  resizable: boolean;                                      // images
};

type PickedFile = { path: string; name: string; size: number };
```

| Command | Arguments | Returns |
| --- | --- | --- |
| `inspect_carrier` | `path` | `CarrierInfo` |
| `carrier_preview` | `path`, `maxSize` (px) | raw bytes (`tauri::ipc::Response`): JPEG thumbnail for images; for audio, `Float32Array` bytes of 256 peak values in 0..1 |
| `stat_files` | `paths` | `PickedFile[]` |
| `estimate_payload` | `text`, `files` | `number` |
| `prepare_carrier` | `path`, `scale` (0 < s ≤ 1), `jobId`, `onProgress` | `{ path, info: CarrierInfo }`, a new file in the app cache |
| `import_begin` | `name` | `importId` (a new file in the app cache) |
| `import_chunk` | `importId`, `data` (base64, ≤ 2 MiB decoded per call) | `void` |
| `import_finish` | `importId` | `PickedFile` |
| `import_abort` | `importId` | `void` (deletes the partial file) |
| `hide` | `request: { carrier, text, files, passphrase, compress }`, `jobId`, `onProgress` | `HideReport` (adds `name`, `extension`); output held in memory |
| `save_output` | `path` | `void` |
| `discard_output` | | `void` |
| `reveal` | `carrier`, `passphrase`, `jobId`, `onProgress` | `RevealedEntry[]` |
| `save_entry` | `index`, `path` | `void` |
| `clear_revealed` | | `void` |
| `cancel_job` | `jobId` | `void` (no-op for an unknown id) |
| `pick_files` (Android only) | `mimeTypes`, `multiple` | `PickedFile[]`; `[]` when the user cancels |
| `pick_save_path` (Android only) | `name`, `mimeType` | `string \| null` |

On desktop the frontend keeps using `@tauri-apps/plugin-dialog` for open and
save dialogs, and Tauri's `onDragDropEvent` for drops.

Progress is throttled to at most ~20 events per second, and `fraction`
reaches 1.0 only on success.

### Core library (`stegano-core`)

```rust
pub enum Stage { Analyzing, Keying, Embedding, Encoding, Verifying, Extracting, Decrypting, Resizing }
pub struct Control<'a> {
    pub cancel: Option<&'a AtomicBool>,
    pub progress: Option<&'a (dyn Fn(Stage, f32) + Sync)>, // stage fraction 0..1
}
pub fn embed_with(data, entries, passphrase, options, ctl: &Control) -> Result<Embedded>;
pub fn extract_with(data, passphrase, ctl: &Control) -> Result<Vec<Entry>>;
pub fn resize_image(data: &[u8], scale: f32, ctl: &Control) -> Result<Vec<u8>>;
Error::Cancelled
```

`embed` and `extract` stay as wrappers with no control. Cancellation is checked
between stages and inside the parallel loops (cost map rows, STC blocks),
so a cancel takes effect within about 100 ms on a phone. Embedding progress
comes from completed STC blocks across all three segments.

`resize_image` keeps the format. JPEG: decode with mozjpeg, apply the EXIF
orientation, resample, re-encode with mozjpeg at quality 92, 4:2:0, keeping
the ICC profile and nothing else (EXIF/GPS dropped). PNG, BMP, TIFF and
lossless WebP: resample with a Lanczos3 filter, keep bit depth and colour type,
keep only ICC and colour-space chunks. Scale 1.0 returns the input unchanged.

## 7. Android fixes

**Keyboard closing.** No device was available to confirm the cause. These are
the likely causes, ranked, with what 0.3 does about each:

1. *Autofill and password-manager overlays.* When a field gets focus, and again
   after each keystroke, the Android autofill framework queries the autofill
   service, which can show a popup window over the WebView. That popup takes
   window focus and the IME hides. This matches "closes a moment after focus"
   and "closes after the first letter". Fix: mark the WebView
   `importantForAutofill = NO_EXCLUDE_DESCENDANTS` (API 26+) in
   `MainActivity.onWebViewCreate`, and `autocomplete="off"` on fields.
   Passphrases should not go into an autofill service anyway.
2. *Edge-to-edge without IME insets.* With targetSdk 36 the activity is always
   edge-to-edge and the WebView is not resized for the keyboard, so the page
   jumps as Chromium scrolls the focused field into view. Fix:
   `windowSoftInputMode="adjustResize"` and a `WindowInsets` listener that pads
   the content view by system bars, display cutout and IME insets, so the
   WebView always fits the visible area.
3. *Old runtime.* Upgrade Tauri, wry and the plugins to the latest releases.

`MainActivity` also logs window-focus changes to logcat (tag `Stegano`), so a
remaining issue can be diagnosed with `adb logcat -s Stegano`.

**Picked files not appearing.** In tauri-plugin-dialog the Android picker
turns every failure into `null`, so the app can't tell "cancelled" from
"failed" and just does nothing. Tauri ≤ 2.10 also kept activity-result
callbacks per activity, so a result could be lost when Android recreated the
activity behind the picker. Large photos also took long enough to inspect,
with only a small "Inspecting…" text, that a pick looked like it hadn't worked.
Fixes:

- An app-local Kotlin plugin (`PickerPlugin`) with `pickFiles` using
  `ACTION_OPEN_DOCUMENT` (taking persistable read permission) that returns
  name and size from `OpenableColumns`, rejects with a real error message, and
  returns an empty list on cancel. `pickSavePath` uses `ACTION_CREATE_DOCUMENT`.
- Upgraded Tauri (singleton plugin manager).
- Immediate feedback in the UI on pick (section 4.1).

## 8. Video carriers (not in 0.3)

Modern video is lossy (H.264/H.265/AV1), so hiding data means changing
quantised coefficients inside the bitstream, as the JPEG carrier does. That
needs a full entropy decoder and re-encoder (CAVLC/CABAC), and because frames
predict from each other, any change in a reference frame drifts through later
frames. Options, by effort:

1. **Audio track of a video** (MP4/MKV with PCM or FLAC audio, or MKV
   remuxing): reuses the audio carrier; capacity is about that of the audio.
   Medium effort (container demux/remux).
2. **Lossless video** (FFV1 in MKV): easy embedding, but such files are rare
   and draw attention.
3. **H.264 coefficient embedding** in non-reference frames only, to avoid
   drift: the real feature, and a research-sized project.

For moving large files, splitting the payload across several image carriers
(already in the README's ideas list) is a cheaper path to the same goal.
