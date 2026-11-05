<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { ProgressIndicator, ProgressRoot } from "reka-ui";
import { formatDuration, percentOf } from "../lib/format";
import ResponsiveSheet from "./ResponsiveSheet.vue";

const props = defineProps<{
    open: boolean;
    title: string;
    label: string;
    fraction: number;
    startedAt: number;
    cancelling: boolean;
}>();
const emit = defineEmits<{ cancel: [] }>();

const elapsed = ref("0:00");
let timer = 0;

function tick(): void {
    if (!props.startedAt) {
        elapsed.value = "0:00";
        return;
    }
    elapsed.value = formatDuration(Date.now() - props.startedAt);
}

watch(
    () => props.open,
    (isOpen) => {
        window.clearInterval(timer);
        if (!isOpen) return;
        tick();
        timer = window.setInterval(tick, 250);
    },
    { immediate: true },
);

onBeforeUnmount(() => window.clearInterval(timer));
</script>

<template>
    <ResponsiveSheet :open="open" :title="title" description="Large files can take a minute." :dismissable="false" @blocked="emit('cancel')">
        <div class="flex flex-col gap-5 pb-1">
            <div>
                <p class="big-pct" aria-hidden="true">{{ percentOf(fraction) }}%</p>
                <p class="mt-2 text-[15px] text-muted" aria-live="polite">{{ label }}</p>
            </div>
            <ProgressRoot
                class="bar-thick"
                :model-value="percentOf(fraction)"
                :max="100"
                :get-value-text="() => `${label}, ${percentOf(fraction)} percent`"
            >
                <ProgressIndicator :style="{ transform: `translateX(-${100 - percentOf(fraction)}%)` }" class="block" />
            </ProgressRoot>
            <div class="flex items-center justify-between">
                <p class="t-caption">{{ elapsed }} elapsed</p>
                <button type="button" class="btn btn-ghost -mr-3" :disabled="cancelling" @click="emit('cancel')">
                    {{ cancelling ? "Cancelling…" : "Cancel" }}
                </button>
            </div>
        </div>
    </ResponsiveSheet>
</template>
