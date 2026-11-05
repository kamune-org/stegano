import { computed, onScopeDispose, ref, watch } from "vue";
import { defineStore } from "pinia";
import { toast } from "vue-sonner";
import {
    discardOutput,
    estimatePayload,
    hide,
    isCancelled,
    normalizeError,
    prepareCarrier,
    saveOutput,
    statFiles,
    type CarrierInfo,
    type HideReport,
    type PickedFile,
    type PreparedCarrier,
    type Scale,
    type Stage,
} from "../api";
import { runJob, type JobHandle } from "../api/jobs";
import {
    confirmOverwrite,
    isAndroid,
    isCarrierFileName,
    isDesktopShell,
    isTauri,
    pickCarrierFile,
    pickOutputPath,
    pickPayloadFiles,
    showInFolder,
} from "../api/pickers";
import { useCarrierLoader } from "../lib/carrier";
import { displayName, fileStem, stageLabel } from "../lib/format";
import { importBytes } from "../lib/importBytes";

type Prepared = { scale: Scale; path: string; info: CarrierInfo };

export const useHideStore = defineStore("hide", () => {
    const carrier = useCarrierLoader();
    const scale = ref<Scale>(1);
    const prepared = ref<Prepared | null>(null);
    const preparing = ref(false);
    const prepareCancelling = ref(false);
    const prepareFraction = ref(0);
    const prepareStage = ref<Stage>("resizing");
    const prepareError = ref("");

    const text = ref("");
    const files = ref<PickedFile[]>([]);
    const passphrase = ref("");
    const confirmPass = ref("");
    const showPass = ref(false);
    const showConfirm = ref(false);
    const compress = ref(true);

    const payload = ref<number | null>(null);
    const estimating = ref(false);

    const phase = ref<"idle" | "running" | "result">("idle");
    const report = ref<HideReport | null>(null);
    const savedPath = ref<string | null>(null);
    const saving = ref(false);
    const jobActive = ref(false);
    const jobCancelling = ref(false);
    const jobFraction = ref(0);
    const jobStage = ref<Stage>("reading");
    const jobStartedAt = ref(0);

    const importActive = ref(false);
    const importCancelling = ref(false);
    const importFraction = ref(0);
    const importLabel = ref("Importing");
    const importStartedAt = ref(0);
    const locks = ref(0);

    let prepareHandle: JobHandle<PreparedCarrier> | null = null;
    let prepareToken = 0;
    let prepareChain: Promise<void> = Promise.resolve();
    let hideHandle: JobHandle<HideReport> | null = null;
    let importStop = false;
    let picking = false;
    let finishing = false;
    let estimateTimer: ReturnType<typeof setTimeout> | undefined;
    let estimateToken = 0;

    const textBytes = computed(() => new TextEncoder().encode(text.value).length);
    const hasText = computed(() => text.value.trim().length > 0);
    const hasPayload = computed(() => hasText.value || files.value.length > 0);
    const frozen = computed(() => jobActive.value || importActive.value || saving.value);
    const busy = computed(() => frozen.value || preparing.value || locks.value > 0);

    const capacityView = computed(() => {
        const base = carrier.info.value;
        if (!base) return null;
        if (scale.value === 1) {
            return { capacity: base.capacity, recommended: base.recommended, approx: false };
        }
        if (prepared.value && prepared.value.scale === scale.value) {
            return {
                capacity: prepared.value.info.capacity,
                recommended: prepared.value.info.recommended,
                approx: false,
            };
        }
        const factor = scale.value * scale.value;
        return {
            capacity: Math.max(0, Math.floor(base.capacity * factor)),
            recommended: Math.max(0, Math.floor(base.recommended * factor)),
            approx: true,
        };
    });

    const effectivePath = computed(() => {
        if (scale.value !== 1 && prepared.value?.scale === scale.value) return prepared.value.path;
        return carrier.path.value;
    });

    const effectiveInfo = computed(() => {
        if (scale.value !== 1 && prepared.value?.scale === scale.value) return prepared.value.info;
        return carrier.info.value;
    });

    const zone = computed<"safe" | "risky" | "large" | null>(() => {
        const view = capacityView.value;
        if (!view || payload.value === null || !hasPayload.value) return null;
        if (payload.value <= view.recommended) return "safe";
        if (payload.value <= view.capacity) return "risky";
        return "large";
    });

    const hideReason = computed(() => {
        if (frozen.value) return "Working…";
        if (!carrier.path.value) return "Choose a cover file";
        if (carrier.status.value === "loading") return "Checking the cover file";
        if (carrier.status.value !== "ready" || !carrier.info.value) return "Could not read the cover file";
        if (scale.value !== 1 && carrier.info.value.resizable) {
            if (prepareError.value) return "Could not resize the image";
            if (preparing.value || prepared.value?.scale !== scale.value) return "Preparing the image";
        }
        if (!hasPayload.value) return "Add a message or a file";
        if (payload.value === null || estimating.value) {
            if (
                payload.value !== null &&
                capacityView.value &&
                payload.value > capacityView.value.capacity
            ) {
                return "Too large for this cover file";
            }
            return "Checking the size";
        }
        if (capacityView.value && payload.value > capacityView.value.capacity) return "Too large for this cover file";
        if (!passphrase.value) return "Enter a passphrase";
        if (passphrase.value !== confirmPass.value) return "Passphrases don't match";
        return "";
    });

    const progressOpen = computed(() => phase.value === "running" || importActive.value);
    const progressFraction = computed(() => (importActive.value ? importFraction.value : jobFraction.value));
    const progressTitle = computed(() => (importActive.value ? importLabel.value : "Hiding your data"));
    const progressLabel = computed(() =>
        importActive.value ? "Reading the file" : stageLabel(jobStage.value),
    );
    const progressCancelling = computed(() =>
        importActive.value ? importCancelling.value : jobCancelling.value,
    );
    const progressStartedAt = computed(() => (importActive.value ? importStartedAt.value : jobStartedAt.value));

    function lock(): void {
        locks.value++;
    }

    function unlock(): void {
        locks.value = Math.max(0, locks.value - 1);
    }

    async function cancelPrepare(): Promise<void> {
        const handle = prepareHandle;
        prepareToken++;
        prepareHandle = null;
        if (!handle) {
            preparing.value = false;
            prepareCancelling.value = false;
            return;
        }
        prepareCancelling.value = true;
        await handle.cancel();
        preparing.value = false;
        prepareCancelling.value = false;
    }

    function queuePrepare(task: () => Promise<void>): void {
        prepareChain = prepareChain.then(task).catch(() => {});
    }

    async function loadCarrier(file: PickedFile): Promise<void> {
        scale.value = 1;
        prepared.value = null;
        prepareError.value = "";
        const loading = carrier.load(file);
        queuePrepare(() => cancelPrepare());
        await Promise.all([loading, prepareChain]);
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

    async function retryCarrier(): Promise<void> {
        if (frozen.value) return;
        await carrier.retry();
    }

    function removeCarrier(): void {
        if (frozen.value) return;
        queuePrepare(() => cancelPrepare());
        scale.value = 1;
        prepared.value = null;
        prepareError.value = "";
        carrier.remove();
    }

    function selectScale(next: Scale): void {
        if (frozen.value) return;
        if (next === scale.value && (next === 1 || (prepared.value?.scale === next && !prepareError.value))) return;
        queuePrepare(() => beginScale(next));
    }

    async function beginScale(next: Scale): Promise<void> {
        await cancelPrepare();
        const token = prepareToken;
        scale.value = next;
        prepareError.value = "";
        prepareFraction.value = 0;
        if (next === 1 || !carrier.info.value?.resizable || !carrier.path.value) {
            prepared.value = null;
            return;
        }
        const source = carrier.path.value;
        preparing.value = true;
        prepareStage.value = "resizing";
        const handle = runJob((jobId, onProgress) =>
            prepareCarrier(source, next, jobId, (progress) => {
                onProgress(progress);
                if (token !== prepareToken) return;
                prepareFraction.value = progress.fraction;
                prepareStage.value = progress.stage;
            }),
        );
        prepareHandle = handle;
        void finishScale(token, next, handle);
    }

    async function finishScale(token: number, next: Scale, handle: JobHandle<PreparedCarrier>): Promise<void> {
        try {
            const result = await handle.promise;
            if (token !== prepareToken) return;
            prepared.value = { scale: next, path: result.path, info: result.info };
        } catch (error) {
            if (token !== prepareToken || isCancelled(error)) return;
            prepareError.value = normalizeError(error).message;
            prepared.value = null;
        } finally {
            if (token === prepareToken) {
                preparing.value = false;
                prepareHandle = null;
                prepareCancelling.value = false;
            }
        }
    }

    function cancelScale(): void {
        if (!prepareHandle || prepareCancelling.value) return;
        prepareCancelling.value = true;
        void prepareHandle.cancel();
    }

    function addPicked(picked: PickedFile[]): void {
        const known = new Set(files.value.map((file) => file.path));
        const fresh = picked.filter((file) => file.path && !known.has(file.path));
        if (fresh.length === 0) return;
        files.value = [...files.value, ...fresh];
    }

    async function addFiles(): Promise<void> {
        if (picking || frozen.value) return;
        picking = true;
        try {
            addPicked(await pickPayloadFiles());
        } catch (error) {
            toast.error(normalizeError(error).message);
        } finally {
            picking = false;
        }
    }

    function removeFile(index: number): void {
        if (frozen.value) return;
        files.value = files.value.filter((_, item) => item !== index);
    }

    async function addFromPaths(paths: string[]): Promise<void> {
        if (frozen.value || paths.length === 0) return;
        try {
            addPicked(await statFiles(paths));
        } catch (error) {
            toast.error(normalizeError(error).message);
        }
    }

    async function dropCarrierPath(filePath: string): Promise<void> {
        if (frozen.value) return;
        await loadCarrier({ path: filePath, name: displayName(filePath), size: 0 });
    }

    async function addBrowserFiles(list: File[], target: "carrier" | "secret"): Promise<void> {
        if (frozen.value || list.length === 0 || isTauri()) return;
        const { mockAdoptFile } = await import("../api/mock");
        if (target === "carrier") {
            const file = list.length === 1 ? list[0] : undefined;
            if (!file || !isCarrierFileName(file.name)) {
                toast.error("Drop one image or audio file here.");
                return;
            }
            await loadCarrier(mockAdoptFile(file));
            return;
        }
        addPicked(list.map((file) => mockAdoptFile(file)));
    }

    function applyGenerated(value: string): void {
        passphrase.value = value;
        confirmPass.value = value;
        showPass.value = true;
        showConfirm.value = true;
    }

    async function useCaptured(name: string, bytes: Uint8Array, label: string): Promise<void> {
        if (frozen.value) return;
        importActive.value = true;
        importStop = false;
        importCancelling.value = false;
        importFraction.value = 0;
        importLabel.value = label;
        importStartedAt.value = Date.now();
        try {
            const file = await importBytes(
                name,
                bytes,
                (fraction) => {
                    importFraction.value = fraction;
                },
                () => importStop,
            );
            await loadCarrier(file);
        } catch (error) {
            if (!isCancelled(error)) toast.error(normalizeError(error).message);
        } finally {
            importActive.value = false;
            importCancelling.value = false;
        }
    }

    function cancelProgress(): void {
        if (importActive.value) {
            if (importCancelling.value) return;
            importCancelling.value = true;
            importStop = true;
            return;
        }
        if (!hideHandle || jobCancelling.value) return;
        jobCancelling.value = true;
        void hideHandle.cancel();
    }

    async function run(): Promise<void> {
        const source = effectivePath.value;
        if (hideReason.value || !source || jobActive.value) return;
        phase.value = "running";
        jobActive.value = true;
        jobCancelling.value = false;
        jobFraction.value = 0;
        jobStage.value = "reading";
        jobStartedAt.value = Date.now();
        savedPath.value = null;
        const handle = runJob((jobId, onProgress) =>
            hide(
                {
                    carrier: source,
                    text: hasText.value ? text.value : null,
                    files: files.value.map((file) => file.path),
                    passphrase: passphrase.value,
                    compress: compress.value,
                },
                jobId,
                (progress) => {
                    onProgress(progress);
                    jobFraction.value = progress.fraction;
                    jobStage.value = progress.stage;
                },
            ),
        );
        hideHandle = handle;
        try {
            report.value = await handle.promise;
            phase.value = "result";
        } catch (error) {
            phase.value = "idle";
            report.value = null;
            if (!isCancelled(error)) toast.error(normalizeError(error).message);
        } finally {
            jobActive.value = false;
            jobCancelling.value = false;
            hideHandle = null;
        }
    }

    async function saveResult(): Promise<void> {
        const current = report.value;
        if (!current || saving.value) return;
        const name = `${fileStem(current.name)}.${current.extension}`;
        let dest: string | null;
        try {
            dest = await pickOutputPath(name, current.extension);
        } catch (error) {
            toast.error(normalizeError(error).message);
            return;
        }
        if (!dest) return;
        if (isDesktopShell() && dest === carrier.path.value) {
            const ok = await confirmOverwrite();
            if (!ok) return;
        }
        saving.value = true;
        try {
            await saveOutput(dest);
            savedPath.value = dest;
        } catch (error) {
            toast.error(normalizeError(error).message);
        } finally {
            saving.value = false;
        }
    }

    async function showSaved(): Promise<void> {
        if (!savedPath.value) return;
        try {
            await showInFolder(savedPath.value);
        } catch (error) {
            toast.error(normalizeError(error).message);
        }
    }

    async function finishResult(): Promise<void> {
        if (finishing || phase.value !== "result") return;
        finishing = true;
        try {
            await discardOutput();
        } catch (error) {
            toast.error(normalizeError(error).message);
        }
        passphrase.value = "";
        confirmPass.value = "";
        showPass.value = false;
        showConfirm.value = false;
        report.value = null;
        savedPath.value = null;
        phase.value = "idle";
        finishing = false;
    }

    watch([text, files], () => {
        if (estimateTimer) clearTimeout(estimateTimer);
        const token = ++estimateToken;
        if (!hasPayload.value) {
            payload.value = null;
            estimating.value = false;
            return;
        }
        estimating.value = true;
        estimateTimer = setTimeout(() => {
            void (async () => {
                try {
                    const size = await estimatePayload(hasText.value ? text.value : null, files.value);
                    if (token !== estimateToken) return;
                    payload.value = size;
                } catch (error) {
                    if (token !== estimateToken) return;
                    payload.value = null;
                    toast.error(normalizeError(error).message);
                } finally {
                    if (token === estimateToken) estimating.value = false;
                }
            })();
        }, 300);
    });

    onScopeDispose(() => {
        if (estimateTimer) clearTimeout(estimateTimer);
        void prepareHandle?.cancel();
    });

    return {
        carrier,
        scale,
        prepared,
        preparing,
        prepareCancelling,
        prepareFraction,
        prepareStage,
        prepareError,
        text,
        files,
        passphrase,
        confirmPass,
        showPass,
        showConfirm,
        compress,
        payload,
        estimating,
        phase,
        report,
        savedPath,
        saving,
        textBytes,
        hasPayload,
        frozen,
        busy,
        capacityView,
        effectiveInfo,
        zone,
        hideReason,
        progressOpen,
        progressFraction,
        progressTitle,
        progressLabel,
        progressCancelling,
        progressStartedAt,
        desktop: computed(() => isDesktopShell()),
        android: computed(() => isAndroid()),
        lock,
        unlock,
        chooseCarrier,
        retryCarrier,
        removeCarrier,
        selectScale,
        cancelScale,
        addFiles,
        removeFile,
        addFromPaths,
        dropCarrierPath,
        addBrowserFiles,
        applyGenerated,
        useCaptured,
        cancelProgress,
        run,
        saveResult,
        showSaved,
        finishResult,
    };
});
