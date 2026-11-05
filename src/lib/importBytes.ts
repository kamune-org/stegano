import {
    importAbort,
    importBegin,
    importChunk,
    importFinish,
    normalizeError,
    type CommandError,
    type PickedFile,
} from "../api";

const CHUNK_BYTES = 2 * 1024 * 1024;

function cancelled(): CommandError {
    return { code: "cancelled", message: "Cancelled" };
}

/** Chunked base64. Each piece is a multiple of 3 bytes so chunks concatenate. */
function bytesToBase64(bytes: Uint8Array): string {
    const piece = 3 * 8192;
    const parts: string[] = [];
    for (let offset = 0; offset < bytes.length; offset += piece) {
        const slice = bytes.subarray(offset, Math.min(bytes.length, offset + piece));
        const numbers = Array.from(slice);
        let binary = "";
        const step = 0x8000;
        for (let index = 0; index < numbers.length; index += step) {
            binary += String.fromCharCode.apply(null, numbers.slice(index, index + step));
        }
        parts.push(btoa(binary));
    }
    return parts.join("");
}

export async function importBytes(
    name: string,
    bytes: Uint8Array,
    onProgress: (fraction: number) => void,
    isCancelled: () => boolean,
): Promise<PickedFile> {
    const importId = await importBegin(name);
    try {
        const total = bytes.byteLength;
        let offset = 0;
        if (total === 0) {
            if (isCancelled()) throw cancelled();
            onProgress(1);
            return await importFinish(importId);
        }
        while (offset < total) {
            if (isCancelled()) throw cancelled();
            const end = Math.min(total, offset + CHUNK_BYTES);
            await importChunk(importId, bytesToBase64(bytes.subarray(offset, end)));
            offset = end;
            onProgress(offset / total);
        }
        if (isCancelled()) throw cancelled();
        return await importFinish(importId);
    } catch (error) {
        await importAbort(importId).catch(() => {});
        throw normalizeError(error);
    }
}
