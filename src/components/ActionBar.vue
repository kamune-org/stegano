<script setup lang="ts">
import { computed } from "vue";
import { formatBytes } from "../lib/format";
import { useHideStore } from "../stores/hide";
import { useRevealStore } from "../stores/reveal";

const props = defineProps<{ mode: "hide" | "reveal" }>();
const hide = useHideStore();
const reveal = useRevealStore();

const reason = computed(() => (props.mode === "hide" ? hide.hideReason : reveal.revealReason));

const zoneWord = computed(() => {
    if (hide.zone === "safe") return "Safe";
    if (hide.zone === "risky") return "Risky";
    if (hide.zone === "large") return "Too large";
    return "";
});

const summary = computed(() => {
    if (props.mode !== "hide" || !hide.capacityView || hide.payload === null) return "";
    const zone = zoneWord.value;
    return zone ? `${formatBytes(hide.payload)} · ${zone}` : formatBytes(hide.payload);
});

const phoneText = computed(() => reason.value || summary.value);

const fill = computed(() => {
    const view = hide.capacityView;
    if (!view || hide.payload === null || view.capacity <= 0) return 0;
    return Math.min(100, (hide.payload / view.capacity) * 100);
});

const showPhoneMeter = computed(
    () => props.mode === "hide" && !reason.value && hide.capacityView !== null && hide.payload !== null,
);

function run(): void {
    if (props.mode === "hide") void hide.run();
    else void reveal.run();
}
</script>

<template>
    <div class="action-bar">
        <div class="min-w-0 flex-1">
            <p class="truncate text-[14px] font-medium" :class="reason ? 'text-muted' : ''" aria-live="polite">
                {{ phoneText }}
            </p>
            <div
                v-if="showPhoneMeter"
                class="meter meter-slim mt-1.5"
                role="progressbar"
                aria-label="Payload size compared with capacity"
                :aria-valuemin="0"
                :aria-valuemax="hide.capacityView?.capacity ?? 0"
                :aria-valuenow="hide.payload ?? 0"
            >
                <div class="meter-fill" :class="hide.zone ?? ''" :style="{ width: fill + '%' }"></div>
            </div>
        </div>
        <button type="button" class="btn btn-primary action-go" :disabled="!!reason" @click="run">
            {{ mode === "hide" ? "Hide" : "Reveal" }}
        </button>
    </div>
</template>
