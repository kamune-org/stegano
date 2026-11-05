/** A fresh ArrayBuffer so Blob accepts bytes from a shared or pooled view. */
export function blobBytes(bytes: Uint8Array): ArrayBuffer {
    const copy = new ArrayBuffer(bytes.byteLength);
    new Uint8Array(copy).set(bytes);
    return copy;
}

/** JPEG starts with FF D8. PNG starts with 89 50 4E 47. */
export function sniffImageMime(bytes: Uint8Array): "image/jpeg" | "image/png" | null {
    if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xd8) return "image/jpeg";
    if (
        bytes.length >= 4 &&
        bytes[0] === 0x89 &&
        bytes[1] === 0x50 &&
        bytes[2] === 0x4e &&
        bytes[3] === 0x47
    ) {
        return "image/png";
    }
    return null;
}

/** 256 little-endian float32 peaks in 0..1. */
export function decodePeaks(bytes: Uint8Array): number[] {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const count = Math.min(256, Math.floor(bytes.byteLength / 4));
    const peaks: number[] = [];
    for (let i = 0; i < count; i++) {
        const value = view.getFloat32(i * 4, true);
        peaks.push(Number.isFinite(value) ? Math.max(0, Math.min(1, Math.abs(value))) : 0);
    }
    return peaks;
}
