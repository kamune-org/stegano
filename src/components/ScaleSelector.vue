<script setup lang="ts">
import { computed } from "vue";
import { ChevronsUpDown } from "@lucide/vue";
import { SCALES, type CarrierInfo, type Scale } from "../api";
import { formatBytes, percentOf, stageLabel } from "../lib/format";
import { useLayout } from "../lib/layout";

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
const { desktop } = useLayout();

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

type Option = (typeof options.value)[number];

function optionLabel(option: Option): string {
    const dims = option.width && option.height ? `${option.width}×${option.height}` : "";
    if (option.value === 1) return dims ? `Original (${dims})` : "Original";
    const capacity = `${option.approx ? "≈ " : ""}${formatBytes(option.capacity)}`;
    return `${Math.round(option.value * 100)}%${dims ? ` (${dims})` : ""} ${capacity}`;
}

function onPick(event: Event): void {
    const value = Number((event.target as HTMLSelectElement).value);
    const next = SCALES.find((item) => item === value);
    if (next !== undefined) choose(next);
}
</script>

<template>
    <div>
        <div v-if="desktop" class="scale-row">
            <label for="scale-select" class="t-label">Size</label>
            <div class="select-wrap">
                <select id="scale-select" class="select" :value="scale" :disabled="disabled" @change="onPick">
                    <option v-for="option in options" :key="option.value" :value="option.value">
                        {{ optionLabel(option) }}
                    </option>
                </select>
                <ChevronsUpDown :size="12" class="select-icon" aria-hidden="true" />
            </div>
            <p v-if="scale !== 1" class="t-caption scale-note">Re-encodes the image and removes metadata.</p>
        </div>
        <template v-else>
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
        </template>
        <div v-if="preparing" class="prepare mt-3" aria-live="polite">
            <div class="prepare-head mb-1.5 flex items-center justify-between text-[13px]">
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
            <button type="button" class="btn btn-ghost prepare-cancel -ml-3 mt-1" :disabled="cancelling" @click="emit('cancel')">
                {{ cancelling ? "Cancelling…" : "Cancel" }}
            </button>
        </div>
        <p v-if="error" class="prepare-error mt-2 text-sm text-danger" role="alert">{{ error }}</p>
    </div>
</template>
