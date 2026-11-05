<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { Plus } from "@lucide/vue";
import type { PickedFile } from "../api";
import { formatBytes } from "../lib/format";
import FileList from "./FileList.vue";

const text = defineModel<string>({ required: true });
const props = defineProps<{
    files: PickedFile[];
    bytes: number;
    hot?: boolean;
    disabled?: boolean;
    browserDrop?: boolean;
}>();
const emit = defineEmits<{ add: []; remove: [index: number]; files: [files: File[]] }>();

const root = ref<HTMLElement | null>(null);
const area = ref<HTMLTextAreaElement | null>(null);
defineExpose({ el: root });

function grow(): void {
    const el = area.value;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 280)}px`;
}

watch(text, () => {
    void nextTick(grow);
});

function onDragOver(event: DragEvent): void {
    if (!props.browserDrop) return;
    event.preventDefault();
}

function onDrop(event: DragEvent): void {
    if (!props.browserDrop) return;
    event.preventDefault();
    const list = event.dataTransfer?.files;
    if (!list || list.length === 0) return;
    emit("files", Array.from(list));
}
</script>

<template>
    <section
        ref="root"
        class="card flex flex-col gap-4"
        :class="{ hot }"
        aria-labelledby="secret-heading"
        @dragover="onDragOver"
        @drop="onDrop"
    >
        <h2 id="secret-heading" class="t-section">Secret</h2>
        <div>
            <div class="mb-2 flex items-baseline justify-between gap-3">
                <label for="hide-message" class="t-label">Message</label>
                <p class="t-caption" aria-hidden="true">{{ formatBytes(bytes) }}</p>
            </div>
            <textarea
                id="hide-message"
                ref="area"
                v-model="text"
                class="field"
                rows="4"
                placeholder="Write a message"
                autocomplete="off"
                :disabled="disabled"
                @input="grow"
            ></textarea>
        </div>
        <div v-if="files.length" class="-my-1">
            <FileList :files="files" :disabled="disabled" @remove="emit('remove', $event)" />
        </div>
        <button type="button" class="btn btn-tonal self-start" :disabled="disabled" @click="emit('add')">
            <Plus :size="18" aria-hidden="true" />
            Add files
        </button>
        <slot />
    </section>
</template>
