import { ref, type Ref } from "vue";
import { cancelJob, isCancelled, type Progress, type Stage } from "./index";

export type JobHandle<T> = {
    jobId: string;
    progress: Ref<Progress>;
    cancelling: Ref<boolean>;
    startedAt: number;
    promise: Promise<T>;
    cancel: () => Promise<void>;
};

export function createJobId(): string {
    if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
    bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function runJob<T>(
    work: (jobId: string, onProgress: (progress: Progress) => void) => Promise<T>,
): JobHandle<T> {
    const jobId = createJobId();
    const progress = ref<Progress>({ stage: "reading" satisfies Stage, fraction: 0 });
    const cancelling = ref(false);
    let settled = false;

    const promise = work(jobId, (next) => {
        const fraction = Math.min(1, Math.max(0, next.fraction));
        if (fraction + 1e-6 < progress.value.fraction) return;
        progress.value = { stage: next.stage, fraction };
    }).finally(() => {
        settled = true;
    });

    return {
        jobId,
        progress,
        cancelling,
        startedAt: Date.now(),
        promise,
        async cancel() {
            if (cancelling.value || settled) return;
            cancelling.value = true;
            try {
                await cancelJob(jobId);
            } catch (error) {
                if (!isCancelled(error)) cancelling.value = true;
            }
        },
    };
}
