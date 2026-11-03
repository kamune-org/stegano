import { invoke } from "@tauri-apps/api/core";
import { open, save, ask } from "@tauri-apps/plugin-dialog";

/*
 * Typed wrappers around the Rust backend. All paths are opaque strings: on
 * desktop they are file system paths, on Android they are content:// URIs.
 * They are only ever handed back to Rust, never read from JS.
 */

export type CarrierKind = "image" | "audio";

export type CarrierInfo = {
    name: string;
    size: number;
    format: string;
    extension: string;
    kind: CarrierKind;
    description: string;
    capacity: number;
    recommended: number;
};

export type FileStat = { path: string; name: string; size: number };

export type HideRequest = {
    carrier: string;
    output: string;
    text: string | null;
    files: string[];
    passphrase: string;
    compress: boolean;
};

export type HideReport = {
    embeddedBytes: number;
    changed: number;
    elements: number;
    capacity: number;
    recommended: number;
    outputSize: number;
};

export type RevealedEntry = {
    index: number;
    kind: "text" | "file";
    name: string;
    size: number;
    modified: number | null;
    text: string | null;
};

export const inspectCarrier = (path: string) =>
    invoke<CarrierInfo>("inspect_carrier", { path });

export const statFiles = (paths: string[]) =>
    invoke<FileStat[]>("stat_files", { paths });

export const estimatePayload = (text: string | null, files: FileStat[]) =>
    invoke<number>("estimate_payload", { text, files });

export const hide = (request: HideRequest) =>
    invoke<HideReport>("hide", { request });

export const reveal = (carrier: string, passphrase: string) =>
    invoke<RevealedEntry[]>("reveal", { carrier, passphrase });

export const saveEntry = (index: number, path: string) =>
    invoke<void>("save_entry", { index, path });

export const clearRevealed = () => invoke<void>("clear_revealed");

/** Backend errors reject with a plain string. */
export function errorMessage(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    return String(e);
}

// ---------------------------------------------------------------- dialogs

const CARRIER_FILTERS = [
    {
        name: "Images and audio",
        extensions: ["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp", "wav", "flac"],
    },
    { name: "Images", extensions: ["png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"] },
    { name: "Audio", extensions: ["wav", "flac"] },
];

export async function pickCarrier(): Promise<string | null> {
    const picked = await open({ multiple: false, directory: false, filters: CARRIER_FILTERS });
    return typeof picked === "string" ? picked : null;
}

export async function pickPayloadFiles(): Promise<string[]> {
    const picked = await open({ multiple: true, directory: false });
    if (!picked) return [];
    return Array.isArray(picked) ? picked : [picked];
}

export function pickSavePath(defaultName: string, extension?: string): Promise<string | null> {
    return save({
        defaultPath: defaultName,
        filters: extension ? [{ name: extension.toUpperCase(), extensions: [extension] }] : undefined,
    });
}

export function confirmOverwrite(): Promise<boolean> {
    return ask("The output location is the same as the carrier file. Overwrite the original?", {
        title: "Overwrite original?",
        kind: "warning",
        okLabel: "Overwrite",
        cancelLabel: "Cancel",
    });
}
