<script setup lang="ts">
import { Check, FolderOpen } from "@lucide/vue";
import type { HideReport } from "../api";
import { changedPercent, fileStem, formatBytes } from "../lib/format";
import ResponsiveSheet from "./ResponsiveSheet.vue";

defineProps<{
    open: boolean;
    report: HideReport | null;
    saving: boolean;
    saved: boolean;
    canShowFolder: boolean;
}>();
const emit = defineEmits<{ save: []; show: []; done: [] }>();
</script>

<template>
    <ResponsiveSheet
        :open="open && !!report"
        bare
        :title="report ? `Hidden in ${report.name}` : 'Hidden'"
        description="Save the new file. Closing this sheet discards it."
        @close="emit('done')"
    >
        <div v-if="report" class="flex flex-col gap-5">
            <div class="flex flex-col items-center gap-3 pt-2 text-center">
                <div class="check-badge" aria-hidden="true">
                    <Check :size="28" :stroke-width="3" />
                </div>
                <h2 class="t-section break-all text-[19px]" aria-hidden="true">Hidden in {{ report.name }}</h2>
            </div>
            <dl class="kv">
                <dt>Hidden data</dt>
                <dd>{{ formatBytes(report.embeddedBytes) }}</dd>
                <dt>Changed</dt>
                <dd>
                    {{ report.changed.toLocaleString() }} of {{ report.elements.toLocaleString() }}
                    ({{ changedPercent(report.changed, report.elements) }}%)
                </dd>
                <dt>Output size</dt>
                <dd>{{ formatBytes(report.outputSize) }}</dd>
                <dt class="sr-only">Suggested name</dt>
                <dd class="sr-only">{{ fileStem(report.name) }}.{{ report.extension }}</dd>
            </dl>
            <p v-if="saved" class="inline-flex items-center justify-center gap-1.5 text-[15px] font-semibold text-mint-ink" role="status">
                <Check :size="16" :stroke-width="3" aria-hidden="true" />
                Saved
            </p>
            <p v-else class="t-caption text-center">Closing this sheet discards the new file.</p>
            <div class="flex flex-col gap-2">
                <button v-if="!saved" type="button" class="btn btn-primary w-full" :disabled="saving" @click="emit('save')">
                    {{ saving ? "Saving…" : "Save…" }}
                </button>
                <button v-if="saved && canShowFolder" type="button" class="btn btn-tonal w-full" @click="emit('show')">
                    <FolderOpen :size="18" aria-hidden="true" />
                    Show in folder
                </button>
                <button type="button" class="btn w-full" :class="saved ? 'btn-primary' : 'btn-ghost'" @click="emit('done')">
                    Done
                </button>
            </div>
        </div>
    </ResponsiveSheet>
</template>
