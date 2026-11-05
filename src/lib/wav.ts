function writeString(bytes: Uint8Array, offset: number, value: string): void {
    for (let i = 0; i < value.length; i++) bytes[offset + i] = value.charCodeAt(i);
}

/** 16-bit PCM mono WAV. Samples are clamped and rounded, with no dither. */
export function encodeWav(chunks: Float32Array[], sampleRate: number): Uint8Array {
    let count = 0;
    for (const chunk of chunks) count += chunk.length;

    const bytes = new Uint8Array(44 + count * 2);
    const view = new DataView(bytes.buffer);
    writeString(bytes, 0, "RIFF");
    view.setUint32(4, 36 + count * 2, true);
    writeString(bytes, 8, "WAVE");
    writeString(bytes, 12, "fmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    writeString(bytes, 36, "data");
    view.setUint32(40, count * 2, true);

    let offset = 44;
    for (const chunk of chunks) {
        for (let i = 0; i < chunk.length; i++) {
            const sample = chunk[i] ?? 0;
            const clamped = Math.max(-1, Math.min(1, sample));
            const quantized = clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767);
            view.setInt16(offset, quantized, true);
            offset += 2;
        }
    }
    return bytes;
}
