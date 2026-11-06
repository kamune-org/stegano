<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { toast, Toaster } from "vue-sonner";
import { useDesktopDragDrop } from "./lib/dragdrop";
import { useTextFieldFocus } from "./lib/keyboard";
import { useLayout } from "./lib/layout";
import { useShortcuts } from "./lib/shortcuts";
import { useHideStore } from "./stores/hide";
import { useRevealStore } from "./stores/reveal";
import ActionBar from "./components/ActionBar.vue";
import AppBar from "./components/AppBar.vue";
import BottomNav from "./components/BottomNav.vue";
import DesktopToolbar from "./components/DesktopToolbar.vue";
import HideView from "./views/HideView.vue";
import RevealView from "./views/RevealView.vue";

type Mode = "hide" | "reveal";

const mode = ref<Mode>("hide");
const hide = useHideStore();
const reveal = useRevealStore();
const keyboard = useTextFieldFocus();
const { desktop } = useLayout();
const locked = computed(() => hide.busy || reveal.busy);
const scrolled = ref(false);
const scroller = ref<HTMLElement | null>(null);
const scrollMemory: Record<Mode, number> = { hide: 0, reveal: 0 };
const hideView = ref<{ carrierElement: () => HTMLElement | null; secretElement: () => HTMLElement | null } | null>(null);
const revealView = ref<{ carrierElement: () => HTMLElement | null } | null>(null);

const { over } = useDesktopDragDrop({
    enabled: () => !locked.value,
    carrier: () =>
        mode.value === "hide" ? (hideView.value?.carrierElement() ?? null) : (revealView.value?.carrierElement() ?? null),
    secret: () => (mode.value === "hide" ? (hideView.value?.secretElement() ?? null) : null),
    onCarrier: (path) => {
        if (mode.value === "hide") void hide.dropCarrierPath(path);
        else void reveal.dropCarrierPath(path);
    },
    onSecret: (paths) => {
        void hide.addFromPaths(paths);
    },
    onReject: (message) => toast.error(message),
});

useShortcuts({
    choose: () => {
        if (mode.value === "hide") void hide.chooseCarrier();
        else void reveal.chooseCarrier();
    },
    addFiles: () => {
        if (mode.value === "hide") void hide.addFiles();
    },
    run: () => {
        if (mode.value === "hide") void hide.run();
        else void reveal.run();
    },
    save: () => {
        if (hide.phase !== "result" || hide.savedPath || hide.saving) return false;
        void hide.saveResult();
        return true;
    },
    mode: (next) => {
        if (!locked.value) mode.value = next;
    },
    blocked: () => locked.value || hide.phase !== "idle" || reveal.progressOpen,
});

function onScroll(event: Event): void {
    const target = event.target;
    if (target instanceof HTMLElement) scrolled.value = target.scrollTop > 2;
}

watch(mode, (next, prev) => {
    if (prev === "reveal" && next !== "reveal") reveal.leave();
    // Each screen keeps its own scroll position.
    if (scroller.value) scrollMemory[prev] = scroller.value.scrollTop;
    void nextTick(() => {
        scroller.value?.scrollTo({ top: scrollMemory[next] });
        scrolled.value = (scroller.value?.scrollTop ?? 0) > 2;
    });
});
</script>

<template>
    <div class="app-shell">
        <DesktopToolbar v-if="desktop" v-model="mode" :disabled="locked" />
        <AppBar v-else :title="mode === 'hide' ? 'Hide' : 'Reveal'" :scrolled="scrolled" />
        <main ref="scroller" class="app-main min-h-0 flex-1 overflow-y-auto" @scroll.passive="onScroll">
            <div class="page">
                <HideView
                    v-show="mode === 'hide'"
                    ref="hideView"
                    :carrier-hot="over === 'carrier' && mode === 'hide'"
                    :secret-hot="over === 'secret' && mode === 'hide'"
                />
                <RevealView
                    v-show="mode === 'reveal'"
                    ref="revealView"
                    :carrier-hot="over === 'carrier' && mode === 'reveal'"
                />
            </div>
        </main>
        <div v-if="!desktop" v-show="!keyboard" class="bars">
            <ActionBar :mode="mode" />
            <BottomNav v-model="mode" :disabled="locked" />
        </div>
        <Toaster
            theme="system"
            position="top-center"
            :offset="{ top: 'calc(env(safe-area-inset-top) + 12px)' }"
            close-button
        />
    </div>
</template>
