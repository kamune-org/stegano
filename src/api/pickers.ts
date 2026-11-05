import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { pickFiles, pickSavePathCommand, statFiles, type PickedFile } from "./index";
import { displayName, extensionOf } from "../lib/format";

export function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function isAndroid(): boolean {
    return /Android/i.test(navigator.userAgent);
}

export function isDesktopShell(): boolean {
    return isTauri() && !isAndroid();
}

export const CARRIER_MIME_TYPES = [
    "image/png",
    "image/jpeg",
    "image/bmp",
    "image/tiff",
    "image/webp",
    "audio/wav",
    "audio/x-wav",
    "audio/wave",
    "audio/flac",
    "audio/x-flac",
];

const CARRIER_EXTENSIONS = ["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp", "wav", "flac"];

const CARRIER_FILTERS = [
    { name: "Images and audio", extensions: CARRIER_EXTENSIONS },
    { name: "Images", extensions: ["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"] },
    { name: "Audio", extensions: ["wav", "flac"] },
];

const MIME_BY_EXTENSION: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    bmp: "image/bmp",
    tif: "image/tiff",
    tiff: "image/tiff",
    webp: "image/webp",
    wav: "audio/wav",
    flac: "audio/flac",
};

export function mimeForExtension(extension: string): string {
    return MIME_BY_EXTENSION[extension.replace(/^\./, "").toLowerCase()] ?? "application/octet-stream";
}

export function isCarrierFileName(name: string): boolean {
    return CARRIER_EXTENSIONS.includes(extensionOf(name).toLowerCase());
}

export function canCaptureMedia(): boolean {
    return typeof navigator !== "undefined" && typeof navigator.mediaDevices?.getUserMedia === "function";
}

export async function pickCarrierFile(): Promise<PickedFile | null> {
    if (!isTauri()) {
        const { mockPickCarrier } = await import("./mock");
        return mockPickCarrier();
    }
    if (isAndroid()) {
        const files = await pickFiles(CARRIER_MIME_TYPES, false);
        return files[0] ?? null;
    }
    const picked = await open({ multiple: false, directory: false, filters: CARRIER_FILTERS });
    if (typeof picked !== "string") return null;
    return { path: picked, name: displayName(picked), size: 0 };
}

export async function pickPayloadFiles(): Promise<PickedFile[]> {
    if (!isTauri()) {
        const { mockPickPayload } = await import("./mock");
        return mockPickPayload();
    }
    if (isAndroid()) return pickFiles(["*/*"], true);
    const picked = await open({ multiple: true, directory: false });
    if (!picked) return [];
    const paths = Array.isArray(picked) ? picked : [picked];
    if (paths.length === 0) return [];
    return statFiles(paths);
}

export async function pickOutputPath(name: string, extension: string): Promise<string | null> {
    const mimeType = mimeForExtension(extension);
    if (!isTauri()) {
        const { mockPickSave } = await import("./mock");
        return mockPickSave(name);
    }
    if (isAndroid()) return pickSavePathCommand(name, mimeType);
    const picked = await save({
        defaultPath: name,
        filters: extension
            ? [{ name: extension.toUpperCase(), extensions: [extension.replace(/^\./, "")] }]
            : undefined,
    });
    return picked ?? null;
}

export function confirmOverwrite(): Promise<boolean> {
    return ask("The output location is the same as the cover file. Overwrite the original?", {
        title: "Overwrite original?",
        kind: "warning",
        okLabel: "Overwrite",
        cancelLabel: "Cancel",
    });
}

export async function showInFolder(path: string): Promise<void> {
    if (!isDesktopShell()) return;
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
}
