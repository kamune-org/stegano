export type CommandErrorCode =
    | "cancelled"
    | "not_found"
    | "too_large"
    | "unsuitable"
    | "unsupported"
    | "corrupt"
    | "io"
    | "other";

export type CommandError = {
    code: CommandErrorCode;
    message: string;
};

export type Stage =
    | "reading"
    | "analyzing"
    | "keying"
    | "embedding"
    | "encoding"
    | "verifying"
    | "extracting"
    | "decrypting"
    | "resizing"
    | "writing";

/** Overall progress, 0..1, monotonic. `fraction` is 1 only on success. */
export type Progress = {
    stage: Stage;
    fraction: number;
};

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
    width: number | null;
    height: number | null;
    durationMs: number | null;
    sampleRate: number | null;
    channels: number | null;
    resizable: boolean;
};

export type PickedFile = {
    path: string;
    name: string;
    size: number;
};

export type HideRequest = {
    carrier: string;
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
    name: string;
    extension: string;
};

export type RevealedEntry = {
    index: number;
    kind: "text" | "file";
    name: string;
    size: number;
    modified: number | null;
    text: string | null;
};

export type PreparedCarrier = {
    path: string;
    info: CarrierInfo;
};

export type Scale = 1 | 0.8 | 0.5 | 0.2;

export const SCALES: readonly Scale[] = [1, 0.8, 0.5, 0.2];

export interface Backend {
    inspectCarrier(path: string): Promise<CarrierInfo>;
    carrierPreview(path: string, maxSize: number): Promise<Uint8Array>;
    statFiles(paths: string[]): Promise<PickedFile[]>;
    estimatePayload(text: string | null, files: PickedFile[]): Promise<number>;
    prepareCarrier(
        path: string,
        scale: number,
        jobId: string,
        onProgress: (progress: Progress) => void,
    ): Promise<PreparedCarrier>;
    importBegin(name: string): Promise<string>;
    importChunk(importId: string, data: string): Promise<void>;
    importFinish(importId: string): Promise<PickedFile>;
    importAbort(importId: string): Promise<void>;
    hide(
        request: HideRequest,
        jobId: string,
        onProgress: (progress: Progress) => void,
    ): Promise<HideReport>;
    saveOutput(path: string): Promise<void>;
    discardOutput(): Promise<void>;
    reveal(
        carrier: string,
        passphrase: string,
        jobId: string,
        onProgress: (progress: Progress) => void,
    ): Promise<RevealedEntry[]>;
    saveEntry(index: number, path: string): Promise<void>;
    clearRevealed(): Promise<void>;
    cancelJob(jobId: string): Promise<void>;
    pickFiles(mimeTypes: string[], multiple: boolean): Promise<PickedFile[]>;
    pickSavePath(name: string, mimeType: string): Promise<string | null>;
}
