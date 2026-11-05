<script setup lang="ts">
import { File, FileAudio, FileImage, FileText, X } from "@lucide/vue";
import type { Component } from "vue";
import type { PickedFile } from "../api";
import { extensionOf, formatBytes } from "../lib/format";

defineProps<{ files: PickedFile[]; disabled?: boolean }>();
const emit = defineEmits<{ remove: [index: number] }>();

function iconFor(name: string): Component {
    const ext = extensionOf(name).toLowerCase();
    if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"].includes(ext)) return FileImage;
    if (["wav", "flac", "mp3", "m4a", "aac", "ogg"].includes(ext)) return FileAudio;
    if (["txt", "md", "json", "csv", "pdf"].includes(ext)) return FileText;
    return File;
}
</script>

<template>
    <ul v-if="files.length" class="rows">
        <li v-for="(file, index) in files" :key="file.path" class="row">
            <span class="row-icon">
                <component :is="iconFor(file.name)" :size="18" aria-hidden="true" />
            </span>
            <div class="min-w-0 flex-1">
                <p class="truncate text-[15px] font-medium">{{ file.name }}</p>
                <p class="t-caption">{{ formatBytes(file.size) }}</p>
            </div>
            <button
                type="button"
                class="btn btn-icon -mr-2 shrink-0"
                :disabled="disabled"
                :aria-label="`Remove ${file.name}`"
                @click="emit('remove', index)"
            >
                <X :size="18" aria-hidden="true" />
            </button>
        </li>
    </ul>
</template>
