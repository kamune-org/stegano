<script setup lang="ts">
import { computed } from "vue";
import { withShortcut } from "../lib/layout";
import { useHideStore } from "../stores/hide";
import { useRevealStore } from "../stores/reveal";
import ModeSwitch from "./ModeSwitch.vue";

const mode = defineModel<"hide" | "reveal">({ required: true });
defineProps<{ disabled?: boolean }>();

const hide = useHideStore();
const reveal = useRevealStore();

const reason = computed(() => (mode.value === "hide" ? hide.hideReason : reveal.revealReason));
const label = computed(() => (mode.value === "hide" ? "Hide…" : "Reveal"));
const tip = computed(() => reason.value || withShortcut(mode.value === "hide" ? "Hide" : "Reveal", "Enter"));

function run(): void {
    if (mode.value === "hide") void hide.run();
    else void reveal.run();
}
</script>

<template>
    <header class="toolbar" data-tauri-drag-region>
        <div class="toolbar-side" data-tauri-drag-region></div>
        <ModeSwitch v-model="mode" :disabled="disabled" />
        <div class="toolbar-side toolbar-end" data-tauri-drag-region>
            <p id="toolbar-hint" class="toolbar-hint" aria-live="polite" data-tauri-drag-region>{{ reason }}</p>
            <span :title="tip">
                <button type="button" class="btn btn-primary toolbar-go" :disabled="!!reason" aria-describedby="toolbar-hint" @click="run">
                    {{ label }}
                </button>
            </span>
        </div>
    </header>
</template>
