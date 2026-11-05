import { onBeforeUnmount, onMounted, watch, type Ref } from "vue";

type Layer = {
    id: number;
    /** When true, Back re-pushes history and asks the layer to handle it. */
    persist: boolean;
    requestClose: () => void;
};

const stack: Layer[] = [];
let sequence = 0;
let listening = false;
const suppress = new Set<number>();

/**
 * `history.back()` is async. A pushState in the same turn sits above the
 * entry being removed, and the late popstate then looks like Back on the
 * new sheet. Hold pushes and further backs until that popstate lands.
 */
let backInFlight = false;
let backToken = 0;
let backTimer = 0;
let pendingBacks = 0;
const pendingPushes: Array<() => void> = [];

function historyId(): number | null {
    const state = history.state as { stegano?: unknown } | null;
    return state && typeof state.stegano === "number" ? state.stegano : null;
}

function clearBackTimer(): void {
    if (!backTimer) return;
    window.clearTimeout(backTimer);
    backTimer = 0;
}

function flushPushes(): void {
    const tasks = pendingPushes.splice(0);
    for (const task of tasks) task();
}

function settleBack(): void {
    if (!backInFlight) return;
    backInFlight = false;
    clearBackTimer();
    backToken += 1;
    if (pendingBacks > 0) {
        pendingBacks -= 1;
        startBack();
        return;
    }
    flushPushes();
}

function startBack(): void {
    backInFlight = true;
    const token = ++backToken;
    clearBackTimer();
    backTimer = window.setTimeout(() => {
        backTimer = 0;
        if (token !== backToken || !backInFlight) return;
        settleBack();
    }, 300);
    history.back();
}

function requestHistoryBack(): void {
    if (backInFlight) {
        pendingBacks += 1;
        return;
    }
    startBack();
}

function onPopState(): void {
    for (const id of [...suppress]) {
        if (!stack.some((layer) => layer.id === id)) suppress.delete(id);
    }
    if (backInFlight) {
        settleBack();
        return;
    }
    const current = historyId();
    while (stack.length > 0 && stack[stack.length - 1]?.id !== current) {
        const layer = stack[stack.length - 1];
        if (!layer) break;
        if (suppress.has(layer.id)) {
            stack.pop();
            suppress.delete(layer.id);
            continue;
        }
        if (layer.persist) {
            history.pushState({ stegano: layer.id }, "");
            layer.requestClose();
            return;
        }
        stack.pop();
        layer.requestClose();
    }
}

function ensureListening(): void {
    if (listening || typeof window === "undefined") return;
    listening = true;
    window.addEventListener("popstate", onPopState);
}

/**
 * Push a history entry while `open` is true so the platform Back button
 * closes this layer instead of leaving the app.
 * `persist` keeps the entry up: Back calls `requestClose` without dismissing.
 */
export function useBackstack(
    open: Ref<boolean>,
    requestClose: () => void,
    options?: { persist?: boolean },
): void {
    const persist = options?.persist ?? false;
    let id = 0;
    let pushed = false;
    let queued: (() => void) | null = null;

    function runPush(): void {
        queued = null;
        if (!open.value) return;
        ensureListening();
        id = ++sequence;
        history.pushState({ stegano: id }, "");
        stack.push({ id, persist, requestClose });
        pushed = true;
    }

    function push(): void {
        if (queued) return;
        if (backInFlight) {
            queued = runPush;
            pendingPushes.push(runPush);
            return;
        }
        runPush();
    }

    function syncClosed(): void {
        if (queued) {
            const task = queued;
            queued = null;
            const index = pendingPushes.indexOf(task);
            if (index !== -1) pendingPushes.splice(index, 1);
        }
        if (!pushed) return;
        const closingId = id;
        pushed = false;
        const index = stack.findIndex((layer) => layer.id === closingId);
        if (index === -1) return;
        stack.splice(index, 1);
        if (historyId() === closingId || backInFlight || pendingBacks > 0) {
            suppress.add(closingId);
            requestHistoryBack();
        }
    }

    watch(open, (isOpen, wasOpen) => {
        if (isOpen && !wasOpen) push();
        else if (!isOpen && wasOpen) syncClosed();
    });

    onMounted(() => {
        if (open.value && !pushed && !queued) push();
    });

    onBeforeUnmount(() => {
        if (pushed || queued) syncClosed();
    });
}
