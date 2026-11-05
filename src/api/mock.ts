import type {
    Backend,
    CarrierInfo,
    CommandError,
    HideReport,
    HideRequest,
    PickedFile,
    PreparedCarrier,
    Progress,
    RevealedEntry,
    Stage,
} from "./types";

const cancelledIds = new Set<string>();
const adopted = new Map<string, File>();
const imports = new Map<string, { name: string; bytes: number }>();

let carrierFlip = false;
let payloadCount = 0;
let importCount = 0;

function wait(ms: number): Promise<void> {
    return new Promise((resolve) => {
        setTimeout(resolve, ms);
    });
}

function extensionOf(name: string): string {
    const base = name.split(/[\\/]/).pop() ?? name;
    const dot = base.lastIndexOf(".");
    return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

function scaleOf(path: string): number {
    const match = /[?&]scale=([0-9.]+)/.exec(path);
    const value = match ? Number(match[1]) : 1;
    return Number.isFinite(value) && value > 0 && value <= 1 ? value : 1;
}

function nameOf(path: string): string {
    const clean = path.split("?")[0] ?? path;
    return clean.split("/").pop() || "file";
}

function isAudioName(name: string): boolean {
    const ext = extensionOf(name);
    return ext === "wav" || ext === "flac" || name.includes("voice") || name.startsWith("recording-");
}

function infoFor(path: string, fallbackName: string, fallbackSize: number): CarrierInfo {
    const scale = scaleOf(path);
    const name = fallbackName || nameOf(path);
    const ext = extensionOf(name);
    if (isAudioName(name)) {
        const flac = ext === "flac";
        return {
            name,
            size: fallbackSize || 7_056_044,
            format: flac ? "FLAC" : "WAV",
            extension: flac ? "flac" : "wav",
            kind: "audio",
            description: "44.1 kHz · stereo · 0:42",
            capacity: 220_000,
            recommended: 48_000,
            width: null,
            height: null,
            durationMs: 42_000,
            sampleRate: 44100,
            channels: 2,
            resizable: false,
        };
    }
    const format =
        ext === "png" ? "PNG" : ext === "webp" ? "WebP" : ext === "bmp" ? "BMP" : ext === "tif" || ext === "tiff" ? "TIFF" : "JPEG";
    const extension = ext === "jpeg" ? "jpg" : ext || "jpg";
    const width = Math.max(1, Math.round(4032 * scale));
    const height = Math.max(1, Math.round(3024 * scale));
    const capacity = Math.max(1, Math.floor(900_000 * scale * scale));
    return {
        name,
        size: fallbackSize || Math.max(1, Math.floor(2_800_000 * scale * scale)),
        format,
        extension,
        kind: "image",
        description: `${width}×${height}`,
        capacity,
        recommended: Math.floor(capacity * 0.25),
        width,
        height,
        durationMs: null,
        sampleRate: null,
        channels: null,
        resizable: true,
    };
}

async function staged(jobId: string, stages: Stage[], onProgress: (progress: Progress) => void): Promise<void> {
    const steps = 30;
    for (let step = 1; step <= steps; step++) {
        await wait(100);
        if (cancelledIds.has(jobId)) {
            cancelledIds.delete(jobId);
            throw { code: "cancelled", message: "Cancelled" } satisfies CommandError;
        }
        const fraction = step / steps;
        const stage = stages[Math.min(stages.length - 1, Math.floor(((step - 1) / steps) * stages.length))] ?? "reading";
        onProgress({ stage, fraction });
    }
}

function peaks(): Uint8Array {
    const samples = new Uint8Array(256 * 4);
    const view = new DataView(samples.buffer);
    for (let i = 0; i < 256; i++) {
        const envelope = Math.sin((i / 255) * Math.PI);
        const value = Math.min(1, Math.abs(Math.sin(i * 0.37) * 0.62 + Math.sin(i * 0.09) * 0.28) * envelope);
        view.setFloat32(i * 4, value, true);
    }
    return samples;
}

async function gradientPng(): Promise<Uint8Array> {
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 480;
    const context = canvas.getContext("2d");
    if (!context) return new Uint8Array();
    const gradient = context.createLinearGradient(0, 0, 640, 480);
    gradient.addColorStop(0, "#5B5BD6");
    gradient.addColorStop(0.55, "#8E7CC3");
    gradient.addColorStop(1, "#F2C14E");
    context.fillStyle = gradient;
    context.fillRect(0, 0, 640, 480);
    context.fillStyle = "rgba(255,255,255,0.22)";
    context.beginPath();
    context.arc(430, 150, 70, 0, Math.PI * 2);
    context.fill();
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) return new Uint8Array();
    return new Uint8Array(await blob.arrayBuffer());
}

async function previewFor(path: string): Promise<Uint8Array> {
    const file = adopted.get(path.split("?")[0] ?? path);
    if (file && file.type.startsWith("image/")) return new Uint8Array(await file.arrayBuffer());
    if (isAudioName(nameOf(path)) || file?.type.startsWith("audio/")) return peaks();
    return gradientPng();
}

export async function mockPickCarrier(): Promise<PickedFile> {
    await wait(220);
    carrierFlip = !carrierFlip;
    if (carrierFlip) return { path: "mock://photo.jpg", name: "photo.jpg", size: 2_800_000 };
    return { path: "mock://voice.wav", name: "voice.wav", size: 7_056_044 };
}

export async function mockPickPayload(): Promise<PickedFile[]> {
    await wait(220);
    const id = payloadCount++;
    return [{ path: `mock://payload/notes-${id}.txt`, name: `notes-${id}.txt`, size: 4096 }];
}

export async function mockPickSave(name: string): Promise<string> {
    await wait(180);
    return `mock://saved/${name}`;
}

export function mockAdoptFile(file: File): PickedFile {
    const path = `mock://drop/${Date.now()}-${payloadCount++}/${file.name}`;
    adopted.set(path, file);
    return { path, name: file.name, size: file.size };
}

export function createMockBackend(): Backend {
    return {
        async inspectCarrier(path) {
            await wait(280);
            const file = adopted.get(path.split("?")[0] ?? path);
            return infoFor(path, file?.name || nameOf(path), file?.size || 0);
        },
        async carrierPreview(path) {
            await wait(240);
            return previewFor(path);
        },
        async statFiles(paths) {
            await wait(120);
            return paths.map((path) => {
                const file = adopted.get(path);
                return { path, name: file?.name || nameOf(path), size: file?.size || 12000 };
            });
        },
        async estimatePayload(text, files) {
            await wait(80);
            const textBytes = text ? new TextEncoder().encode(text).length : 0;
            const fileBytes = files.reduce((sum, file) => sum + file.size, 0);
            return textBytes + fileBytes + files.length * 64;
        },
        async prepareCarrier(path, scale, jobId, onProgress) {
            await staged(jobId, ["reading", "analyzing", "resizing", "writing"], onProgress);
            const base = infoFor(path, nameOf(path), 0);
            const prepared = `${path.split("?")[0]}?scale=${scale}`;
            const result: PreparedCarrier = { path: prepared, info: infoFor(prepared, base.name, base.size) };
            return result;
        },
        async importBegin(name) {
            await wait(40);
            const importId = `mock-import-${++importCount}`;
            imports.set(importId, { name, bytes: 0 });
            return importId;
        },
        async importChunk(importId, data) {
            await wait(30);
            const record = imports.get(importId);
            if (!record) throw { code: "io", message: "The import is no longer there." } satisfies CommandError;
            record.bytes += Math.floor((data.length * 3) / 4);
        },
        async importFinish(importId) {
            await wait(40);
            const record = imports.get(importId);
            if (!record) throw { code: "io", message: "The import is no longer there." } satisfies CommandError;
            imports.delete(importId);
            const path = `mock://import/${record.name}`;
            return { path, name: record.name, size: record.bytes };
        },
        async importAbort(importId) {
            imports.delete(importId);
        },
        async hide(request: HideRequest, jobId, onProgress) {
            const info = infoFor(request.carrier, nameOf(request.carrier), 0);
            await staged(jobId, ["reading", "analyzing", "keying", "embedding", "encoding", "verifying", "writing"], onProgress);
            const textBytes = request.text ? new TextEncoder().encode(request.text).length : 0;
            const report: HideReport = {
                embeddedBytes: textBytes + request.files.length * 4096,
                changed: 120034,
                elements: info.kind === "image" ? (info.width ?? 1) * (info.height ?? 1) : 1_852_200,
                capacity: info.capacity,
                recommended: info.recommended,
                outputSize: info.size + 2048,
                name: info.name,
                extension: info.extension,
            };
            return report;
        },
        async saveOutput() {
            await wait(160);
        },
        async discardOutput() {
            await wait(40);
        },
        async reveal(_carrier, passphrase, jobId, onProgress) {
            await staged(jobId, ["reading", "extracting", "decrypting"], onProgress);
            if (passphrase !== "test") {
                throw {
                    code: "not_found",
                    message: "Nothing was found with that passphrase.",
                } satisfies CommandError;
            }
            const entries: RevealedEntry[] = [
                {
                    index: 0,
                    kind: "text",
                    name: "message",
                    size: 32,
                    modified: null,
                    text: "Hello from Stegano.\nThis line stays on its own.",
                },
                {
                    index: 1,
                    kind: "file",
                    name: "notes.txt",
                    size: 2048,
                    modified: 1_700_000_000,
                    text: null,
                },
                {
                    index: 2,
                    kind: "file",
                    name: "map.png",
                    size: 84021,
                    modified: 1_700_001_000,
                    text: null,
                },
            ];
            return entries;
        },
        async saveEntry() {
            await wait(140);
        },
        async clearRevealed() {
            await wait(30);
        },
        async cancelJob(jobId) {
            cancelledIds.add(jobId);
        },
        async pickFiles(mimeTypes, multiple) {
            const wantsMedia = mimeTypes.some((mime) => mime.startsWith("image/") || mime.startsWith("audio/"));
            if (multiple || !wantsMedia) return mockPickPayload();
            return [await mockPickCarrier()];
        },
        async pickSavePath(name) {
            return mockPickSave(name);
        },
    };
}
