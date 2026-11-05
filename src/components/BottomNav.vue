<script setup lang="ts">
import { Eye, EyeOff } from "@lucide/vue";

const mode = defineModel<"hide" | "reveal">({ required: true });
defineProps<{ disabled?: boolean }>();
</script>

<template>
    <nav class="bottom-nav" role="tablist" aria-label="Mode">
        <button
            type="button"
            role="tab"
            class="nav-btn"
            aria-controls="panel-hide"
            :aria-selected="mode === 'hide'"
            :disabled="disabled && mode !== 'hide'"
            @click="mode = 'hide'"
        >
            <EyeOff :size="22" :stroke-width="mode === 'hide' ? 2.25 : 1.75" aria-hidden="true" />
            Hide
        </button>
        <button
            type="button"
            role="tab"
            class="nav-btn"
            aria-controls="panel-reveal"
            :aria-selected="mode === 'reveal'"
            :disabled="disabled && mode !== 'reveal'"
            @click="mode = 'reveal'"
        >
            <Eye :size="22" :stroke-width="mode === 'reveal' ? 2.25 : 1.75" aria-hidden="true" />
            Reveal
        </button>
    </nav>
</template>
