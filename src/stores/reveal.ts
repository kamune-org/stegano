import { computed, onScopeDispose, ref, watch } from "vue";
import { defineStore } from "pinia";
import { toast } from "vue-sonner";
import {
    clearRevealed,
    isCancelled,
    normalizeError,
    reveal,
    saveEntry,
    type RevealedEntry,
    type Stage,
} from "../api";
import { runJob, type JobHandle } from "../api/jobs";
import { isCarrierFileName, isTauri, pickCarrierFile, pickOutputPath } from "../api/pickers";
import { useCarrierLoader } from "../lib/carrier";
import { displayName, extensionOf, safeFileName, stageLabel } from "../lib/format";

export const useRevealStore = defineStore("reveal", () => {
    const carrier = useCarrierLoader();
    const passphrase = ref("");
    const showPass = ref(false);
    const fieldError = ref("");
    const entries = ref<RevealedEntry[]>([]);
    const revealed = ref(false);
    const copiedIndex = ref<number | null>(null);
    const savedIndexes = ref<Set<number>>(new Set());
    const jobActive = ref(false);
    const jobCancelling = ref(false);
    const jobFraction = ref(0);
    const jobStage = ref<Stage>("reading");
    const jobStartedAt = ref(0);
    const saving = ref(false);
    let handle: JobHandle<RevealedEntry[]> | null = null;
    let picking = false;
    let copyTimer = 0;

    const frozen = computed(() => jobActive.value || saving.value);
    const busy = computed(() => frozen.value);

    const revealReason = computed(() => {
        if (frozen.value) return "Working…";
        if (!carrier.path.value) return "Choose a cover file";
        if (carrier.status.value === "loading") return "Checking the cover file";
        if (carrier.status.value !== "ready" || !carrier.info.value) return "Could not read the cover file";
        if (!passphrase.value) return "Enter a passphrase";
        return "";
    });

    async function clearResults(clearPass: boolean): Promise<void> {
        entries.value = [];
        revealed.value = false;
        fieldError.value = "";
        copiedIndex.value = null;
        savedIndexes.value = new Set();
        if (clearPass) {
            passphrase.value = "";
            showPass.value = false;
        }
        try {
            await clearRevealed();
        } catch (error) {
            toast.error(normalizeError(error).message);
        }
    }

    async function loadCarrier(file: { path: string; name: string; size: number }): Promise<void> {
        await clearResults(false);
        await carrier.load(file);
    }

    async function chooseCarrier(): Promise<void> {
        if (picking || frozen.value) return;
        picking = true;
        try {
            const picked = await pickCarrierFile();
            if (picked) await loadCarrier(picked);
        } catch (error) {
            toast.error(normalizeError(error).message);
        } finally {
            picking = false;
        }
    }

    async function dropCarrierPath(filePath: string): Promise<void> {
        if (frozen.value) return;
        await loadCarrier({ path: filePath, name: displayName(filePath), size: 0 });
    }

    async function addBrowserFiles(list: File[]): Promise<void> {
        if (frozen.value || isTauri()) return;
        const file = list.length === 1 ? list[0] : undefined;
        if (!file || !isCarrierFileName(file.name)) {
            toast.error("Drop one image or audio file.");
            return;
        }
        const { mockAdoptFile } = await import("../api/mock");
        await loadCarrier(mockAdoptFile(file));
    }

    async function run(): Promise<void> {
        const source = carrier.path.value;
        if (revealReason.value || !source || jobActive.value) return;
        fieldError.value = "";
        entries.value = [];
        revealed.value = false;
        savedIndexes.value = new Set();
        jobActive.value = true;
        jobCancelling.value = false;
        jobFraction.value = 0;
        jobStage.value = "reading";
        jobStartedAt.value = Date.now();
        const job = runJob((jobId, onProgress) =>
            reveal(source, passphrase.value, jobId, (progress) => {
                onProgress(progress);
                jobFraction.value = progress.fraction;
                jobStage.value = progress.stage;
            }),
        );
        handle = job;
        try {
            entries.value = await job.promise;
            revealed.value = true;
            passphrase.value = "";
            showPass.value = false;
        } catch (error) {
            const command = normalizeError(error);
            if (command.code === "not_found") fieldError.value = command.message;
            else if (!isCancelled(error)) toast.error(command.message);
        } finally {
            jobActive.value = false;
            jobCancelling.value = false;
            handle = null;
        }
    }

    function cancelProgress(): void {
        if (!handle || jobCancelling.value) return;
        jobCancelling.value = true;
        void handle.cancel();
    }

    async function copyEntry(entry: RevealedEntry, area: HTMLTextAreaElement | null): Promise<void> {
        if (entry.text === null) return;
        try {
            await navigator.clipboard.writeText(entry.text);
            copiedIndex.value = entry.index;
            window.clearTimeout(copyTimer);
            copyTimer = window.setTimeout(() => {
                if (copiedIndex.value === entry.index) copiedIndex.value = null;
            }, 2000);
        } catch {
            area?.focus();
            area?.select();
            toast("The message is selected. Copy it from there.");
        }
    }

    async function saveFile(entry: RevealedEntry): Promise<void> {
        if (saving.value) return;
        const name = safeFileName(entry.name);
        let dest: string | null;
        try {
            dest = await pickOutputPath(name, extensionOf(name));
        } catch (error) {
            toast.error(normalizeError(error).message);
            return;
        }
        if (!dest) return;
        saving.value = true;
        try {
            await saveEntry(entry.index, dest);
            savedIndexes.value = new Set(savedIndexes.value).add(entry.index);
            toast.success(`Saved ${name}.`);
        } catch (error) {
            toast.error(normalizeError(error).message);
        } finally {
            saving.value = false;
        }
    }

    function leave(): void {
        if (jobActive.value) return;
        void clearResults(true);
    }

    watch(passphrase, () => {
        fieldError.value = "";
    });

    onScopeDispose(() => {
        window.clearTimeout(copyTimer);
        void clearRevealed().catch(() => {});
    });

    return {
        carrier,
        passphrase,
        showPass,
        fieldError,
        entries,
        revealed,
        copiedIndex,
        savedIndexes,
        jobActive,
        jobCancelling,
        jobFraction,
        jobStage,
        jobStartedAt,
        saving,
        frozen,
        busy,
        revealReason,
        progressOpen: jobActive,
        progressLabel: computed(() => stageLabel(jobStage.value)),
        chooseCarrier,
        retryCarrier: () => carrier.retry(),
        removeCarrier: () => {
            if (frozen.value) return;
            void clearResults(false);
            carrier.remove();
        },
        dropCarrierPath,
        addBrowserFiles,
        run,
        cancelProgress,
        copyEntry,
        saveFile,
        clearAll: () => clearResults(true),
        leave,
    };
});
