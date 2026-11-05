<script setup lang="ts">
import { computed } from "vue";
import { Eye, EyeOff } from "@lucide/vue";

const mode = defineModel<"hide" | "reveal">({ required: true });
defineProps<{ disabled?: boolean }>();

const index = computed(() => (mode.value === "hide" ? 0 : 1));

function choose(next: "hide" | "reveal"): void {
    mode.value = next;
}
</script>

<template>
    <div class="seg seg-pill" :style="{ '--n': 2, '--i': index }" role="tablist" aria-label="Mode">
        <button
            id="tab-hide"
            type="button"
            role="tab"
            :aria-selected="mode === 'hide'"
            aria-controls="panel-hide"
            :disabled="disabled && mode !== 'hide'"
            @click="choose('hide')"
        >
            <EyeOff :size="15" aria-hidden="true" />
            Hide
        </button>
        <button
            id="tab-reveal"
            type="button"
            role="tab"
            :aria-selected="mode === 'reveal'"
            aria-controls="panel-reveal"
            :disabled="disabled && mode !== 'reveal'"
            @click="choose('reveal')"
        >
            <Eye :size="15" aria-hidden="true" />
            Reveal
        </button>
    </div>
</template>
