import { onBeforeUnmount, onMounted, ref, watch, type Ref } from "vue";
import { isAndroid } from "../api/pickers";

const NON_TEXT = new Set([
    "button",
    "checkbox",
    "radio",
    "range",
    "file",
    "submit",
    "reset",
    "color",
    "hidden",
]);

function matchesCoarse(): boolean {
    return typeof window !== "undefined" && window.matchMedia("(pointer: coarse)").matches;
}

/** True when the primary pointer is coarse. Updates if the pointer type changes. */
export function useCoarsePointer(): Ref<boolean> {
    const coarse = ref(matchesCoarse());
    let query: MediaQueryList | null = null;

    function onChange(): void {
        coarse.value = query?.matches ?? false;
    }

    onMounted(() => {
        query = window.matchMedia("(pointer: coarse)");
        coarse.value = query.matches;
        query.addEventListener("change", onChange);
    });

    onBeforeUnmount(() => {
        query?.removeEventListener("change", onChange);
    });

    return coarse;
}

function isTextField(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    if (target.isContentEditable) return true;
    if (target instanceof HTMLTextAreaElement) return !target.readOnly && !target.disabled;
    if (target instanceof HTMLInputElement) {
        return !target.readOnly && !target.disabled && !NON_TEXT.has(target.type);
    }
    return false;
}

/** True while a text field is focused on a phone (coarse pointer or Android). */
export function useTextFieldFocus(): Ref<boolean> {
    const focused = ref(false);
    const coarse = useCoarsePointer();
    const android = isAndroid();
    let timer = 0;

    function hidesBars(): boolean {
        return coarse.value || android;
    }

    function onFocusIn(event: FocusEvent): void {
        if (!hidesBars() || !isTextField(event.target)) return;
        window.clearTimeout(timer);
        focused.value = true;
    }

    function onFocusOut(): void {
        if (!hidesBars()) return;
        window.clearTimeout(timer);
        timer = window.setTimeout(() => {
            if (!isTextField(document.activeElement)) focused.value = false;
        }, 120);
    }

    watch(coarse, () => {
        if (!hidesBars()) focused.value = false;
    });

    onMounted(() => {
        document.addEventListener("focusin", onFocusIn);
        document.addEventListener("focusout", onFocusOut);
    });

    onBeforeUnmount(() => {
        window.clearTimeout(timer);
        document.removeEventListener("focusin", onFocusIn);
        document.removeEventListener("focusout", onFocusOut);
    });

    return focused;
}
