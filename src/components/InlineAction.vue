<script setup lang="ts">
import { computed } from "vue";
import { useHideStore } from "../stores/hide";
import { useRevealStore } from "../stores/reveal";

const props = defineProps<{ mode: "hide" | "reveal" }>();
const hide = useHideStore();
const reveal = useRevealStore();

const reason = computed(() => (props.mode === "hide" ? hide.hideReason : reveal.revealReason));

function run(): void {
    if (props.mode === "hide") void hide.run();
    else void reveal.run();
}
</script>

<template>
    <div class="hidden flex-col gap-2.5 pt-2 md:flex">
        <p class="t-caption min-h-[1.3em] text-center" aria-live="polite">{{ reason }}</p>
        <button type="button" class="btn btn-primary w-full" :disabled="!!reason" @click="run">
            {{ mode === "hide" ? "Hide" : "Reveal" }}
        </button>
    </div>
</template>
