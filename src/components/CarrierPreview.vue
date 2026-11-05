<script setup lang="ts">
import type { CarrierKind } from "../api";

defineProps<{
    kind: CarrierKind | null;
    url: string | null;
    peaks: number[] | null;
    loading: boolean;
    name: string;
}>();
</script>

<template>
    <div>
        <div v-if="loading" class="skeleton aspect-[16/10] w-full !rounded-none" aria-hidden="true"></div>
        <div v-else-if="url" class="preview">
            <img :src="url" :alt="name ? `Preview of ${name}` : 'Cover file preview'" />
        </div>
        <div v-else-if="peaks && peaks.length" class="preview-audio">
            <svg
                viewBox="0 0 256 64"
                preserveAspectRatio="none"
                role="img"
                :aria-label="name ? `Waveform of ${name}` : 'Audio waveform'"
            >
                <rect
                    v-for="(peak, index) in peaks"
                    :key="index"
                    :x="index + 0.19"
                    :y="32 - Math.max(peak, 0.04) * 30"
                    width="0.62"
                    :height="Math.max(2, peak * 60)"
                    fill="currentColor"
                />
            </svg>
        </div>
        <p v-else-if="kind" class="preview grid h-24 place-items-center text-sm text-muted">No preview for this file.</p>
    </div>
</template>
