const ALPHABET = "23456789abcdefghjkmnpqrstuvwxyz";

export type StrengthTone = "none" | "weak" | "medium" | "strong";

export type PassphraseStrength = {
    level: number;
    label: string;
    tone: StrengthTone;
};

/** Same scoring as Stegano 0.2: length, case, digits, symbols. */
export function passphraseStrength(passphrase: string): PassphraseStrength {
    if (!passphrase) return { level: 0, label: "", tone: "none" };

    let score = 0;
    if (passphrase.length >= 6) score++;
    if (passphrase.length >= 10) score++;
    if (passphrase.length >= 14) score++;
    if (/[a-z]/.test(passphrase) && /[A-Z]/.test(passphrase)) score++;
    if (/\d/.test(passphrase)) score++;
    if (/[^a-zA-Z0-9]/.test(passphrase)) score++;

    if (score <= 2) return { level: score, label: "Weak", tone: "weak" };
    if (score <= 4) return { level: score, label: "Medium", tone: "medium" };
    return { level: score, label: "Strong", tone: "strong" };
}

/**
 * Five groups of four characters from an unambiguous alphabet.
 * Rejection sampling avoids modulo bias.
 */
export function generatePassphrase(): string {
    const chars: string[] = [];
    const buf = new Uint8Array(64);
    // Start past the end so the first iteration fills the buffer: an unfilled
    // buffer is all zeros and would yield "2222-2222-…" every time.
    let index = buf.length;
    const span = ALPHABET.length;
    const limit = 256 - (256 % span);
    while (chars.length < 20) {
        if (index >= buf.length) {
            crypto.getRandomValues(buf);
            index = 0;
        }
        const n = buf[index] ?? 0;
        index++;
        if (n < limit) chars.push(ALPHABET[n % span] ?? "");
    }
    const groups: string[] = [];
    for (let group = 0; group < 5; group++) {
        groups.push(chars.slice(group * 4, group * 4 + 4).join(""));
    }
    return groups.join("-");
}
