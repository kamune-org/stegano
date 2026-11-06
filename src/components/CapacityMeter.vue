<script setup lang="ts">
import { computed } from "vue";
import { formatBytes } from "../lib/format";

const props = defineProps<{
    payload: number | null;
    capacity: number;
    recommended: number;
    estimating: boolean;
    approx: boolean;
    zone: "safe" | "risky" | "large" | null;
}>();

const fill = computed(() => {
    if (props.payload === null || props.capacity <= 0) return 0;
    return Math.min(100, (props.payload / props.capacity) * 100);
});

const marker = computed(() =>
    props.capacity > 0 ? Math.min(100, (props.recommended / props.capacity) * 100) : 0,
);

const zoneLabel = computed(() => {
    if (props.zone === "safe") return "Safe";
    if (props.zone === "risky") return "Risky";
    if (props.zone === "large") return "Too large";
    return "";
});

const payloadLabel = computed(() => {
    if (props.payload === null) return props.estimating ? "Checking size…" : "0 B";
    return formatBytes(props.payload);
});

const live = computed(() => {
    const cap = `${props.approx ? "about " : ""}${formatBytes(props.capacity)}`;
    const zone = zoneLabel.value ? ` ${zoneLabel.value}.` : "";
    return `Payload ${payloadLabel.value}.${zone} Capacity ${cap}. Safe up to ${formatBytes(props.recommended)}.`;
});
</script>

<template>
    <div class="cap flex flex-col gap-2.5 pt-1" role="group" aria-label="Capacity">
        <div class="cap-row flex items-baseline justify-between gap-3 text-[14px]">
            <p class="min-w-0 truncate text-muted">
                <span class="font-semibold text-ink">{{ payloadLabel }}</span>
                of {{ approx ? "≈ " : "" }}{{ formatBytes(recommended) }} safe
            </p>
            <p v-if="zoneLabel" class="shrink-0 font-semibold" :class="`tone-${zone}`">{{ zoneLabel }}</p>
        </div>
        <div
            class="meter"
            role="progressbar"
            aria-label="Payload size compared with capacity"
            :aria-valuemin="0"
            :aria-valuemax="capacity"
            :aria-valuenow="payload ?? 0"
        >
            <div class="meter-fill" :class="zone ?? ''" :style="{ width: fill + '%' }"></div>
            <div class="meter-marker" :style="{ left: marker + '%' }" aria-hidden="true"></div>
        </div>
        <p class="t-caption cap-holds">Holds up to {{ approx ? "≈ " : "" }}{{ formatBytes(capacity) }} in total.</p>
        <p class="sr-only" aria-live="polite">{{ live }}</p>
    </div>
</template>
