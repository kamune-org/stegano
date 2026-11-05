export function formatBytes(n: number): string {
    if (!Number.isFinite(n) || n < 0) return "-";
    if (n < 1024) return `${n} B`;
    const units = ["KiB", "MiB", "GiB"];
    let v = n / 1024;
    let i = 0;
    while (v >= 1024 && i < units.length - 1) {
        v /= 1024;
        i++;
    }
    return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatDate(unixSeconds: number): string {
    return new Date(unixSeconds * 1000).toLocaleString();
}

/** Best-effort display name for an opaque path or content:// URI. */
export function displayName(path: string): string {
    let p = path;
    try {
        p = decodeURIComponent(p);
    } catch {
        /* keep raw */
    }
    const last = p.split(/[\\/:]/).filter(Boolean).pop();
    return last ?? path;
}

export function fileStem(name: string): string {
    const base = name.split(/[\\/]/).pop() ?? name;
    const i = base.lastIndexOf(".");
    return i > 0 ? base.slice(0, i) : base;
}

/** Strip any directory components from a name that came from untrusted data. */
export function safeFileName(name: string): string {
    return name.split(/[\\/]/).filter(Boolean).pop() || "file";
}

export function extensionOf(name: string): string {
    const base = safeFileName(name);
    const i = base.lastIndexOf(".");
    return i > 0 ? base.slice(i + 1) : "";
}

/** `ms` as `m:ss`. */
export function formatDuration(ms: number): string {
    if (!Number.isFinite(ms) || ms < 0) return "-";
    const total = Math.round(ms / 1000);
    const minutes = Math.floor(total / 60);
    const seconds = total % 60;
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function stampedName(prefix: string, extension: string): string {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    const stamp = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
    return `${prefix}-${stamp}.${extension}`;
}

const STAGE_LABELS: Record<string, string> = {
    reading: "Reading the file",
    analyzing: "Analysing the cover file",
    keying: "Deriving the key",
    embedding: "Hiding the data",
    encoding: "Encoding the file",
    verifying: "Checking the result",
    extracting: "Reading hidden data",
    decrypting: "Decrypting",
    resizing: "Resizing the image",
    writing: "Writing the file",
};

export function stageLabel(stage: string): string {
    return STAGE_LABELS[stage] ?? "Working";
}

export function percentOf(fraction: number): number {
    if (!Number.isFinite(fraction) || fraction <= 0) return 0;
    if (fraction >= 1) return 100;
    return Math.floor(fraction * 100);
}

export function changedPercent(changed: number, elements: number): string {
    if (elements <= 0) return "0";
    const pct = (changed / elements) * 100;
    if (pct < 0.01 && changed > 0) return "<0.01";
    return pct.toFixed(2);
}

export function channelLabel(channels: number | null): string {
    if (channels === 1) return "mono";
    if (channels === 2) return "stereo";
    if (channels === null) return "";
    return `${channels} ch`;
}
