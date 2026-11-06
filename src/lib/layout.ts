import { computed, ref, watchEffect } from "vue";
import { isAndroid } from "../api/pickers";

/**
 * The one place that decides between the phone UI and the desktop UI.
 *
 * - Density follows the input device: a fine pointer (mouse, trackpad) gets
 *   desktop density at every window width; coarse pointers and Android always
 *   get the phone UI.
 * - Layout follows width: desktop density at least `SPLIT_MIN_WIDTH` px wide
 *   uses the two-pane split layout, narrower windows fall back to one column.
 *
 * CSS keys off `data-ui="desktop|phone"` and `data-split="true|false"` on the
 * root element (see `styles/desktop.css`); components use `useLayout()`.
 */
export const SPLIT_MIN_WIDTH = 860;

function query(text: string): MediaQueryList | null {
    return typeof window === "undefined" || !window.matchMedia ? null : window.matchMedia(text);
}

const fineQuery = query("(pointer: fine)");
const wideQuery = query(`(min-width: ${SPLIT_MIN_WIDTH}px)`);
const fine = ref(fineQuery?.matches ?? false);
const wide = ref(wideQuery?.matches ?? false);
const android = typeof navigator !== "undefined" && isAndroid();

fineQuery?.addEventListener("change", (event) => {
    fine.value = event.matches;
});
wideQuery?.addEventListener("change", (event) => {
    wide.value = event.matches;
});

const desktop = computed(() => fine.value && !android);
const split = computed(() => desktop.value && wide.value);
const mac = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent) && !/iPhone|iPad/i.test(navigator.userAgent);

/** Reflect the layout on the root element. Call once, before mounting. */
export function installLayout(): void {
    watchEffect(() => {
        const root = document.documentElement;
        root.dataset.ui = desktop.value ? "desktop" : "phone";
        root.dataset.split = split.value ? "true" : "false";
        root.dataset.platform = mac ? "mac" : "other";
    });
}

export function useLayout() {
    return { desktop, split, mac };
}

/** The platform's shortcut text, e.g. `⌘O` / `Ctrl+O`, `⇧⌘O` / `Ctrl+Shift+O`. */
export function shortcutLabel(key: string, shift = false): string {
    if (mac) return `${shift ? "⇧" : ""}⌘${key === "Enter" ? "↩" : key}`;
    return `Ctrl+${shift ? "Shift+" : ""}${key === "Enter" ? "Enter" : key}`;
}

/** A tooltip with its shortcut, e.g. "Choose cover file (⌘O)". Phones get the bare label. */
export function withShortcut(label: string, key: string, shift = false): string | undefined {
    return desktop.value ? `${label} (${shortcutLabel(key, shift)})` : undefined;
}
