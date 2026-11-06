<script setup lang="ts">
import { Check, FolderOpen } from "@lucide/vue";
import type { HideReport } from "../api";
import { changedPercent, fileStem, formatBytes } from "../lib/format";
import { useLayout, withShortcut } from "../lib/layout";
import ResponsiveSheet from "./ResponsiveSheet.vue";

defineProps<{
    open: boolean;
    report: HideReport | null;
    saving: boolean;
    saved: boolean;
    canShowFolder: boolean;
}>();
const emit = defineEmits<{ save: []; show: []; done: [] }>();
const { desktop } = useLayout();
</script>

<template>
    <ResponsiveSheet
        :open="open && !!report"
        bare
        :title="report ? `Hidden in ${report.name}` : 'Hidden'"
        :description="`Save the new file. Closing this ${desktop ? 'dialog' : 'sheet'} discards it.`"
        @close="emit('done')"
    >
        <div v-if="report" class="flex flex-col gap-5">
            <div class="result-head flex flex-col items-center gap-3 pt-2 text-center">
                <div class="check-badge" aria-hidden="true">
                    <Check :size="desktop ? 16 : 28" :stroke-width="3" />
                </div>
                <h2 class="t-section result-title break-all text-[19px]" aria-hidden="true">Hidden in {{ report.name }}</h2>
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
            <p v-if="saved" class="result-saved inline-flex items-center justify-center gap-1.5 t-body font-semibold text-mint-ink" role="status">
                <Check :size="16" :stroke-width="3" aria-hidden="true" />
                Saved
            </p>
            <p v-else class="t-caption result-note text-center">Closing this {{ desktop ? "dialog" : "sheet" }} discards the new file.</p>
            <div v-if="desktop" class="dlg-actions">
                <button v-if="saved && canShowFolder" type="button" class="btn btn-tonal" @click="emit('show')">
                    <FolderOpen :size="14" aria-hidden="true" />
                    Show in Folder
                </button>
                <button type="button" class="btn" :class="saved ? 'btn-primary' : 'btn-tonal'" @click="emit('done')">Done</button>
                <button
                    v-if="!saved"
                    type="button"
                    class="btn btn-primary"
                    :disabled="saving"
                    :title="withShortcut('Save', 'S')"
                    @click="emit('save')"
                >
                    {{ saving ? "Saving…" : "Save…" }}
                </button>
            </div>
            <div v-else class="flex flex-col gap-2">
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
