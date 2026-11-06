import { onBeforeUnmount, onMounted } from "vue";
import { useLayout } from "./layout";

export type Shortcuts = {
    /** Cmd/Ctrl+O */
    choose: () => void;
    /** Shift+Cmd/Ctrl+O */
    addFiles: () => void;
    /** Cmd/Ctrl+Enter */
    run: () => void;
    /** Cmd/Ctrl+S, only while a result dialog is up. Returns true when it handled the key. */
    save: () => boolean;
    /** Cmd/Ctrl+1 / +2 */
    mode: (mode: "hide" | "reveal") => void;
    /** True while a dialog, capture sheet or job owns the keyboard. */
    blocked: () => boolean;
};

/**
 * Desktop keyboard shortcuts (Cmd on macOS, Ctrl elsewhere). Copy, paste and
 * select-all are never touched, and nothing fires during IME composition.
 */
export function useShortcuts(shortcuts: Shortcuts): void {
    const { desktop, mac } = useLayout();

    function onKeyDown(event: KeyboardEvent): void {
        if (!desktop.value || event.isComposing || event.keyCode === 229 || event.altKey) return;
        const mod = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
        if (!mod) return;
        const key = event.key.toLowerCase();

        if (key === "s" && !event.shiftKey) {
            if (shortcuts.save()) event.preventDefault();
            return;
        }
        if (shortcuts.blocked()) return;

        if (key === "o") {
            event.preventDefault();
            if (event.shiftKey) shortcuts.addFiles();
            else shortcuts.choose();
        } else if (key === "enter" && !event.shiftKey) {
            event.preventDefault();
            shortcuts.run();
        } else if ((key === "1" || key === "2") && !event.shiftKey) {
            event.preventDefault();
            shortcuts.mode(key === "1" ? "hide" : "reveal");
        }
    }

    onMounted(() => window.addEventListener("keydown", onKeyDown));
    onBeforeUnmount(() => window.removeEventListener("keydown", onKeyDown));
}
