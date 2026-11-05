<script setup lang="ts">
import { computed } from "vue";
import { SCALES, type CarrierInfo, type Scale } from "../api";
import { formatBytes, percentOf, stageLabel } from "../lib/format";

const scale = defineModel<Scale>({ required: true });
const props = defineProps<{
    info: CarrierInfo;
    prepared: CarrierInfo | null;
    preparing: boolean;
    cancelling: boolean;
    fraction: number;
    stage: string;
    error: string;
    disabled?: boolean;
}>();
const emit = defineEmits<{ cancel: [] }>();

const options = computed(() =>
    SCALES.map((value) => {
        const measured = value !== 1 && scale.value === value ? props.prepared : null;
        if (measured) {
            return {
                value,
                width: measured.width,
                height: measured.height,
                capacity: measured.capacity,
                approx: false,
            };
        }
        const width = props.info.width === null ? null : Math.max(1, Math.round(props.info.width * value));
        const height = props.info.height === null ? null : Math.max(1, Math.round(props.info.height * value));
        return {
            value,
            width,
            height,
            capacity: Math.floor(props.info.capacity * value * value),
            approx: value !== 1,
        };
    }),
);

const index = computed(() => Math.max(0, SCALES.indexOf(scale.value)));
const selected = computed(() => options.value[index.value]);

function choose(value: Scale): void {
    if (props.disabled) return;
    scale.value = value;
}
</script>

<template>
    <div>
        <p id="scale-label" class="t-label mb-2">Size</p>
        <div
            class="seg"
            :style="{ '--n': options.length, '--i': index }"
            role="radiogroup"
            aria-labelledby="scale-label"
        >
            <button
                v-for="option in options"
                :key="option.value"
                type="button"
                role="radio"
                :aria-checked="scale === option.value"
                :disabled="disabled"
                @click="choose(option.value)"
            >
                {{ Math.round(option.value * 100) }}%
            </button>
        </div>
        <p v-if="selected" class="t-caption mt-2.5" aria-live="polite">
            <template v-if="scale === 1">Original size<template v-if="selected.width && selected.height"> · {{ selected.width }}×{{ selected.height }}</template></template>
            <template v-else>
                <template v-if="selected.width && selected.height">{{ selected.width }}×{{ selected.height }} · </template>
                {{ selected.approx ? "≈ " : "" }}{{ formatBytes(selected.capacity) }} capacity
            </template>
        </p>
        <p v-if="scale !== 1" class="t-caption mt-1">Resizing re-encodes the image and removes metadata.</p>
        <div v-if="preparing" class="mt-3" aria-live="polite">
            <div class="mb-1.5 flex items-center justify-between text-[13px]">
                <span class="text-muted">{{ stageLabel(stage) }}</span>
                <span class="font-medium">{{ percentOf(fraction) }}%</span>
            </div>
            <div
                class="meter"
                role="progressbar"
                :aria-valuenow="percentOf(fraction)"
                aria-valuemin="0"
                aria-valuemax="100"
                :aria-valuetext="`${stageLabel(stage)}, ${percentOf(fraction)} percent`"
            >
                <div class="meter-fill" style="background: var(--primary)" :style="{ width: percentOf(fraction) + '%' }"></div>
            </div>
            <button type="button" class="btn btn-ghost -ml-3 mt-1" :disabled="cancelling" @click="emit('cancel')">
                {{ cancelling ? "Cancelling…" : "Cancel" }}
            </button>
        </div>
        <p v-if="error" class="mt-2 text-sm text-danger" role="alert">{{ error }}</p>
    </div>
</template>
