import { Channel, invoke } from "@tauri-apps/api/core";
import type {
    Backend,
    CarrierInfo,
    CommandError,
    CommandErrorCode,
    HideReport,
    HideRequest,
    PickedFile,
    PreparedCarrier,
    Progress,
    RevealedEntry,
    Stage,
} from "./types";

export type {
    Backend,
    CarrierInfo,
    CarrierKind,
    CommandError,
    CommandErrorCode,
    HideReport,
    HideRequest,
    PickedFile,
    PreparedCarrier,
    Progress,
    RevealedEntry,
    Scale,
    Stage,
} from "./types";
export { SCALES } from "./types";

const CODES: readonly CommandErrorCode[] = [
    "cancelled",
    "not_found",
    "too_large",
    "unsuitable",
    "unsupported",
    "corrupt",
    "io",
    "other",
];

const STAGES: readonly Stage[] = [
    "reading",
    "analyzing",
    "keying",
    "embedding",
    "encoding",
    "verifying",
    "extracting",
    "decrypting",
    "resizing",
    "writing",
];

function isCode(value: string): value is CommandErrorCode {
    return (CODES as readonly string[]).includes(value);
}

function isStage(value: string): value is Stage {
    return (STAGES as readonly string[]).includes(value);
}

export function normalizeError(error: unknown): CommandError {
    if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
        const code = (error as { code: unknown }).code;
        const message = (error as { message: unknown }).message;
        if (typeof code === "string" && typeof message === "string" && isCode(code)) {
            return { code, message };
        }
        if (typeof message === "string") return { code: "other", message };
    }
    if (typeof error === "string") return { code: "other", message: error };
    if (error instanceof Error) return { code: "other", message: error.message || "Something went wrong" };
    return { code: "other", message: "Something went wrong" };
}

export function isCancelled(error: unknown): boolean {
    return normalizeError(error).code === "cancelled";
}

function sanitizeProgress(message: Progress): Progress {
    const rawStage = typeof message?.stage === "string" ? message.stage : "";
    const rawFraction = typeof message?.fraction === "number" ? message.fraction : 0;
    return {
        stage: isStage(rawStage) ? rawStage : "reading",
        fraction: Number.isFinite(rawFraction) ? Math.min(1, Math.max(0, rawFraction)) : 0,
    };
}

function channelFor(handler: (progress: Progress) => void): Channel<Progress> {
    return new Channel<Progress>((message) => {
        handler(sanitizeProgress(message));
    });
}

/** IPC: Tauri may return an ArrayBuffer, a view, or a byte array. */
function toBytes(raw: unknown): Uint8Array {
    if (raw instanceof ArrayBuffer) return new Uint8Array(raw);
    if (raw instanceof Uint8Array) return new Uint8Array(raw);
    if (ArrayBuffer.isView(raw)) {
        return new Uint8Array(raw.buffer, raw.byteOffset, raw.byteLength);
    }
    if (Array.isArray(raw) && raw.every((item) => typeof item === "number")) {
        return Uint8Array.from(raw);
    }
    throw { code: "other", message: "The preview was not image or audio data." } satisfies CommandError;
}

async function wrap<T>(run: () => Promise<T>): Promise<T> {
    try {
        return await run();
    } catch (error) {
        throw normalizeError(error);
    }
}

const tauriBackend: Backend = {
    inspectCarrier(path) {
        return wrap(() => invoke<CarrierInfo>("inspect_carrier", { path }));
    },
    carrierPreview(path, maxSize) {
        return wrap(async () => {
            const raw: unknown = await invoke("carrier_preview", { path, maxSize });
            return toBytes(raw);
        });
    },
    statFiles(paths) {
        return wrap(() => invoke<PickedFile[]>("stat_files", { paths }));
    },
    estimatePayload(text, files) {
        return wrap(() => invoke<number>("estimate_payload", { text, files }));
    },
    prepareCarrier(path, scale, jobId, onProgress) {
        return wrap(() =>
            invoke<PreparedCarrier>("prepare_carrier", {
                path,
                scale,
                jobId,
                onProgress: channelFor(onProgress),
            }),
        );
    },
    importBegin(name) {
        return wrap(() => invoke<string>("import_begin", { name }));
    },
    importChunk(importId, data) {
        return wrap(() => invoke<void>("import_chunk", { importId, data }));
    },
    importFinish(importId) {
        return wrap(() => invoke<PickedFile>("import_finish", { importId }));
    },
    importAbort(importId) {
        return wrap(() => invoke<void>("import_abort", { importId }));
    },
    hide(request, jobId, onProgress) {
        return wrap(() =>
            invoke<HideReport>("hide", {
                request,
                jobId,
                onProgress: channelFor(onProgress),
            }),
        );
    },
    saveOutput(path) {
        return wrap(() => invoke<void>("save_output", { path }));
    },
    discardOutput() {
        return wrap(() => invoke<void>("discard_output"));
    },
    reveal(carrier, passphrase, jobId, onProgress) {
        return wrap(() =>
            invoke<RevealedEntry[]>("reveal", {
                carrier,
                passphrase,
                jobId,
                onProgress: channelFor(onProgress),
            }),
        );
    },
    saveEntry(index, path) {
        return wrap(() => invoke<void>("save_entry", { index, path }));
    },
    clearRevealed() {
        return wrap(() => invoke<void>("clear_revealed"));
    },
    cancelJob(jobId) {
        return wrap(() => invoke<void>("cancel_job", { jobId }));
    },
    pickFiles(mimeTypes, multiple) {
        return wrap(() => invoke<PickedFile[]>("pick_files", { mimeTypes, multiple }));
    },
    pickSavePath(name, mimeType) {
        return wrap(() => invoke<string | null>("pick_save_path", { name, mimeType }));
    },
};

let backendPromise: Promise<Backend> | null = null;

function loadBackend(): Promise<Backend> {
    if (!backendPromise) {
        backendPromise =
            typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
                ? Promise.resolve(tauriBackend)
                : import("./mock").then((mod) => mod.createMockBackend());
    }
    return backendPromise;
}

async function use<T>(fn: (backend: Backend) => Promise<T>): Promise<T> {
    const backend = await loadBackend();
    return fn(backend);
}

export const inspectCarrier = (path: string) => use((backend) => backend.inspectCarrier(path));

export const carrierPreview = (path: string, maxSize: number) =>
    use((backend) => backend.carrierPreview(path, maxSize));

export const statFiles = (paths: string[]) => use((backend) => backend.statFiles(paths));

export const estimatePayload = (text: string | null, files: PickedFile[]) =>
    use((backend) => backend.estimatePayload(text, files));

export const prepareCarrier = (
    path: string,
    scale: number,
    jobId: string,
    onProgress: (progress: Progress) => void,
) => use((backend) => backend.prepareCarrier(path, scale, jobId, onProgress));

export const importBegin = (name: string) => use((backend) => backend.importBegin(name));

export const importChunk = (importId: string, data: string) =>
    use((backend) => backend.importChunk(importId, data));

export const importFinish = (importId: string) => use((backend) => backend.importFinish(importId));

export const importAbort = (importId: string) => use((backend) => backend.importAbort(importId));

export const hide = (request: HideRequest, jobId: string, onProgress: (progress: Progress) => void) =>
    use((backend) => backend.hide(request, jobId, onProgress));

export const saveOutput = (path: string) => use((backend) => backend.saveOutput(path));

export const discardOutput = () => use((backend) => backend.discardOutput());

export const reveal = (
    carrier: string,
    passphrase: string,
    jobId: string,
    onProgress: (progress: Progress) => void,
) => use((backend) => backend.reveal(carrier, passphrase, jobId, onProgress));

export const saveEntry = (index: number, path: string) =>
    use((backend) => backend.saveEntry(index, path));

export const clearRevealed = () => use((backend) => backend.clearRevealed());

export const cancelJob = (jobId: string) => use((backend) => backend.cancelJob(jobId));

export const pickFiles = (mimeTypes: string[], multiple: boolean) =>
    use((backend) => backend.pickFiles(mimeTypes, multiple));

export const pickSavePathCommand = (name: string, mimeType: string) =>
    use((backend) => backend.pickSavePath(name, mimeType));
