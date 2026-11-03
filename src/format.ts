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
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(0, i) : name;
}

/** Strip any directory components from a name that came from untrusted data. */
export function safeFileName(name: string): string {
    return name.split(/[\\/]/).filter(Boolean).pop() || "file";
}
