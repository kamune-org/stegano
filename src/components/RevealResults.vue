<script setup lang="ts">
import { ref } from "vue";
import { Check, Copy, FileText } from "@lucide/vue";
import type { RevealedEntry } from "../api";
import { formatBytes, formatDate, safeFileName } from "../lib/format";
import { useLayout } from "../lib/layout";

defineProps<{
    entries: RevealedEntry[];
    copied: number | null;
    saved: Set<number>;
    disabled?: boolean;
}>();
const emit = defineEmits<{ copy: [entry: RevealedEntry, area: HTMLTextAreaElement | null]; save: [entry: RevealedEntry]; clear: [] }>();

const { desktop } = useLayout();
const areas = ref<Record<number, HTMLTextAreaElement | null>>({});

function bindArea(index: number) {
    return (element: unknown) => {
        areas.value[index] = element instanceof HTMLTextAreaElement ? element : null;
    };
}
</script>

<template>
    <section class="card reveal-card flex flex-col gap-4" aria-labelledby="results-heading">
        <div class="flex items-center justify-between gap-3">
            <div class="flex min-w-0 items-center gap-2.5">
                <h2 id="results-heading" class="focus-target t-section" tabindex="-1">Hidden content</h2>
                <span class="badge">
                    <Check :size="13" :stroke-width="3" aria-hidden="true" />
                    Revealed
                </span>
            </div>
            <button type="button" class="btn btn-ghost -mr-3 shrink-0" :disabled="disabled" @click="emit('clear')">Clear</button>
        </div>
        <p v-if="entries.length === 0" class="t-body text-muted">The hidden payload is empty.</p>
        <article v-for="entry in entries.filter((item) => item.kind === 'text')" :key="entry.index" class="flex flex-col gap-2">
            <div class="flex items-center justify-between gap-3">
                <label :for="`entry-${entry.index}`" class="t-label msg-label">Message</label>
                <button v-if="desktop" type="button" class="btn btn-tonal btn-sm" @click="emit('copy', entry, areas[entry.index] ?? null)">
                    <Check v-if="copied === entry.index" :size="13" aria-hidden="true" />
                    <Copy v-else :size="13" aria-hidden="true" />
                    {{ copied === entry.index ? "Copied" : "Copy" }}
                </button>
            </div>
            <textarea
                :id="`entry-${entry.index}`"
                :ref="bindArea(entry.index)"
                class="field mono whitespace-pre-wrap"
                readonly
                rows="6"
                :value="entry.text ?? ''"
                autocomplete="off"
            ></textarea>
            <button v-if="!desktop" type="button" class="btn btn-tonal self-start" @click="emit('copy', entry, areas[entry.index] ?? null)">
                <Check v-if="copied === entry.index" :size="18" aria-hidden="true" />
                <Copy v-else :size="18" aria-hidden="true" />
                {{ copied === entry.index ? "Copied" : "Copy" }}
            </button>
        </article>
        <ul v-if="entries.some((entry) => entry.kind === 'file')" class="rows">
            <li v-for="entry in entries.filter((item) => item.kind === 'file')" :key="entry.index" class="row">
                <span class="row-icon">
                    <FileText :size="18" aria-hidden="true" />
                </span>
                <div class="row-main min-w-0 flex-1">
                    <p class="row-name truncate t-body font-medium">{{ safeFileName(entry.name) }}</p>
                    <p class="t-caption row-size">
                        {{ formatBytes(entry.size) }}
                        <template v-if="entry.modified !== null"> · modified {{ formatDate(entry.modified) }}</template>
                    </p>
                </div>
                <button
                    type="button"
                    class="btn btn-tonal btn-sm shrink-0"
                    :disabled="disabled"
                    :aria-label="`Save ${safeFileName(entry.name)}`"
                    @click="emit('save', entry)"
                >
                    {{ saved.has(entry.index) ? "Saved" : "Save…" }}
                </button>
            </li>
        </ul>
    </section>
</template>
