import { onBeforeUnmount, onMounted, ref, type Ref } from "vue";
import { isCarrierFileName, isDesktopShell } from "../api/pickers";

export type DropTarget = "carrier" | "secret" | null;

type DragPoint = { x: number; y: number };

function contains(element: HTMLElement | null, x: number, y: number): boolean {
    if (!element) return false;
    const rect = element.getBoundingClientRect();
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function toCss(position: DragPoint): DragPoint {
    const scale = window.devicePixelRatio || 1;
    return { x: position.x / scale, y: position.y / scale };
}

export function useDesktopDragDrop(options: {
    enabled: () => boolean;
    carrier: () => HTMLElement | null;
    secret: () => HTMLElement | null;
    onCarrier: (path: string) => void;
    onSecret: (paths: string[]) => void;
    onReject: (message: string) => void;
}): { over: Ref<DropTarget> } {
    const over = ref<DropTarget>(null);
    let unlisten: (() => void) | undefined;
    let stopped = false;

    onMounted(() => {
        if (!isDesktopShell()) return;
        void import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
            const stop = await getCurrentWebview().onDragDropEvent((event) => {
                const payload = event.payload;
                if (!options.enabled()) {
                    over.value = null;
                    return;
                }
                if (payload.type === "leave") {
                    over.value = null;
                    return;
                }
                const point = toCss(payload.position);
                const target: DropTarget = contains(options.carrier(), point.x, point.y)
                    ? "carrier"
                    : contains(options.secret(), point.x, point.y)
                      ? "secret"
                      : null;
                if (payload.type === "drop") {
                    over.value = null;
                    const only = payload.paths.length === 1 ? payload.paths[0] : undefined;
                    if (target === "carrier") {
                        if (only && isCarrierFileName(only)) options.onCarrier(only);
                        else options.onReject("Drop one image or audio file here.");
                    } else if (target === "secret" && payload.paths.length > 0) {
                        options.onSecret(payload.paths);
                    }
                    return;
                }
                over.value = target;
            });
            if (stopped) stop();
            else unlisten = stop;
        });
    });

    onBeforeUnmount(() => {
        stopped = true;
        unlisten?.();
    });

    return { over };
}
