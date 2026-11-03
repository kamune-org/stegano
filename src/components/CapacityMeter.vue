<script setup lang="ts">
import { computed } from "vue";
import { formatBytes } from "../format";

const props = defineProps<{
    /** Payload size before compression, or null while unknown. */
    payload: number | null;
    capacity: number;
    recommended: number;
    estimating: boolean;
}>();

type Zone = "good" | "warn" | "bad";

const zone = computed<Zone | null>(() => {
    if (props.payload === null) return null;
    if (props.payload <= props.recommended) return "good";
    if (props.payload <= props.capacity) return "warn";
    return "bad";
});

const fillPct = computed(() => {
    if (props.payload === null || props.capacity <= 0) return 0;
    return Math.min(100, (props.payload / props.capacity) * 100);
});

const markerPct = computed(() =>
    props.capacity > 0 ? Math.min(100, (props.recommended / props.capacity) * 100) : 0,
);

const message = computed(() => {
    switch (zone.value) {
        case "good":
            return "Low detection risk.";
        case "warn":
            return "Fits, but is easier to detect. Smaller payloads are safer.";
        case "bad":
            return "Too large for this carrier. Remove content or pick a bigger carrier.";
        default:
            return "";
    }
});
</script>

<template>
    <div class="meter" role="group" aria-label="Capacity">
        <div class="meter-head">
            <span>
                Payload:
                <strong>{{ payload === null ? (estimating ? "calculating…" : "-") : formatBytes(payload) }}</strong>
            </span>
            <span class="muted">Capacity {{ formatBytes(capacity) }}</span>
        </div>
        <div
            class="meter-bar"
            role="progressbar"
            aria-label="Payload size relative to capacity"
            :aria-valuemin="0"
            :aria-valuemax="capacity"
            :aria-valuenow="payload ?? 0"
        >
            <div class="meter-fill" :class="zone ?? ''" :style="{ width: fillPct + '%' }"></div>
            <div class="meter-marker" :style="{ left: markerPct + '%' }" aria-hidden="true"></div>
        </div>
        <div class="meter-foot muted">
            <span>Recommended up to {{ formatBytes(recommended) }}</span>
        </div>
        <p v-if="message" class="meter-msg" :class="zone ?? ''" aria-live="polite">{{ message }}</p>
        <p class="muted small">Compression, if enabled, may shrink the payload below this estimate.</p>
    </div>
</template>
