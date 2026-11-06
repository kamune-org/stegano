import { onScopeDispose, ref } from "vue";
import { carrierPreview, inspectCarrier, normalizeError, statFiles, type CarrierInfo, type PickedFile } from "../api";
import { isAndroid, isTauri } from "../api/pickers";
import { blobBytes, decodePeaks, sniffImageMime } from "./preview";

/** Longest side of image previews, in pixels: sharp in the desktop preview pane on retina screens and on high-density phones. */
const PREVIEW_SIZE = 1280;

export type CarrierStatus = "empty" | "loading" | "ready" | "error";

export function useCarrierLoader() {
    const status = ref<CarrierStatus>("empty");
    const path = ref<string | null>(null);
    const name = ref("");
    const size = ref(0);
    const info = ref<CarrierInfo | null>(null);
    const error = ref("");
    const previewUrl = ref<string | null>(null);
    const peaks = ref<number[] | null>(null);
    const previewLoading = ref(false);
    let token = 0;

    function clearPreview(): void {
        if (previewUrl.value) URL.revokeObjectURL(previewUrl.value);
        previewUrl.value = null;
        peaks.value = null;
    }

    function applyPreview(bytes: Uint8Array, kind: CarrierInfo["kind"] | null): void {
        clearPreview();
        const mime = sniffImageMime(bytes);
        if (kind === "audio") {
            peaks.value = decodePeaks(bytes);
            return;
        }
        if (mime) {
            previewUrl.value = URL.createObjectURL(new Blob([blobBytes(bytes)], { type: mime }));
            return;
        }
        if (kind === "image") return;
        if (bytes.byteLength >= 16) peaks.value = decodePeaks(bytes);
    }

    async function load(file: PickedFile): Promise<void> {
        const mine = ++token;
        path.value = file.path;
        name.value = file.name;
        size.value = file.size;
        info.value = null;
        error.value = "";
        status.value = "loading";
        previewLoading.value = true;
        clearPreview();

        const wantStat = isTauri() && !isAndroid() && file.size === 0;
        const [statResult, infoResult, previewResult] = await Promise.allSettled([
            wantStat ? statFiles([file.path]) : Promise.resolve(null),
            inspectCarrier(file.path),
            carrierPreview(file.path, PREVIEW_SIZE),
        ]);
        if (mine !== token) return;

        if (statResult.status === "fulfilled" && statResult.value && statResult.value[0]) {
            name.value = statResult.value[0].name;
            size.value = statResult.value[0].size;
        }
        if (infoResult.status === "fulfilled") {
            info.value = infoResult.value;
            name.value = infoResult.value.name;
            size.value = infoResult.value.size;
            status.value = "ready";
        } else {
            status.value = "error";
            error.value = normalizeError(infoResult.reason).message;
        }
        previewLoading.value = false;
        if (previewResult.status === "fulfilled") applyPreview(previewResult.value, info.value?.kind ?? null);
    }

    function remove(): void {
        token++;
        status.value = "empty";
        path.value = null;
        name.value = "";
        size.value = 0;
        info.value = null;
        error.value = "";
        previewLoading.value = false;
        clearPreview();
    }

    function retry(): Promise<void> {
        if (!path.value) return Promise.resolve();
        return load({ path: path.value, name: name.value, size: size.value });
    }

    onScopeDispose(() => {
        token++;
        clearPreview();
    });

    return { status, path, name, size, info, error, previewUrl, peaks, previewLoading, load, remove, retry };
}
