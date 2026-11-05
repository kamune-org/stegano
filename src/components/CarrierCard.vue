<script setup lang="ts">
import { computed, ref } from "vue";
import { Camera, ChevronDown, FolderOpen, Mic, RefreshCw, Trash2 } from "@lucide/vue";
import { isAndroid } from "../api/pickers";
import { useCoarsePointer } from "../lib/keyboard";
import {
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuPortal,
    DropdownMenuRoot,
    DropdownMenuTrigger,
} from "reka-ui";
import type { CarrierInfo } from "../api";
import { channelLabel, formatBytes, formatDuration } from "../lib/format";
import { useBackstack } from "../lib/backstack";
import CarrierPreview from "./CarrierPreview.vue";
import PixelMark from "./PixelMark.vue";

const props = defineProps<{
    status: "empty" | "loading" | "ready" | "error";
    name: string;
    info: CarrierInfo | null;
    error: string;
    previewUrl: string | null;
    peaks: number[] | null;
    previewLoading: boolean;
    hot?: boolean;
    disabled?: boolean;
    allowCapture?: boolean;
    browserDrop?: boolean;
    holds?: string;
}>();

const emit = defineEmits<{
    choose: [];
    remove: [];
    retry: [];
    photo: [];
    record: [];
    files: [files: File[]];
}>();

const root = ref<HTMLElement | null>(null);
const menuOpen = ref(false);
const coarse = useCoarsePointer();
const dropAvailable = computed(() => !isAndroid() && !coarse.value);
useBackstack(menuOpen, () => {
    menuOpen.value = false;
});

defineExpose({ el: root });

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

const meta = computed(() => {
    const info = props.info;
    if (!info) return "";
    const detail =
        info.kind === "image"
            ? info.width && info.height
                ? `${info.width}×${info.height}`
                : ""
            : [info.durationMs !== null ? formatDuration(info.durationMs) : "", channelLabel(info.channels)]
                  .filter(Boolean)
                  .join(" · ");
    return [info.format, detail, formatBytes(info.size)].filter(Boolean).join(" · ");
});
</script>

<template>
    <section
        ref="root"
        class="card"
        :class="[status === 'empty' ? 'dropzone' : 'card-flush', { hot }]"
        aria-labelledby="carrier-heading"
        :aria-busy="status === 'loading'"
        @dragover="onDragOver"
        @drop="onDrop"
    >
        <template v-if="status === 'empty'">
            <PixelMark />
            <h2 id="carrier-heading" class="t-section mt-5">
                {{ dropAvailable ? "Drop a cover file here" : "Choose a cover file" }}
            </h2>
            <p class="mt-1 max-w-[30ch] text-[15px] text-muted">The photo or audio your secret hides inside.</p>
            <div class="source-grid" :style="{ gridTemplateColumns: `repeat(${allowCapture ? 3 : 1}, minmax(0, 1fr))` }">
                <button type="button" class="btn btn-tonal tile" :class="{ 'tile-wide': !allowCapture }" :disabled="disabled" @click="emit('choose')">
                    <FolderOpen :size="24" aria-hidden="true" />
                    <span>Choose file</span>
                </button>
                <button v-if="allowCapture" type="button" class="btn btn-tonal tile" :disabled="disabled" @click="emit('photo')">
                    <Camera :size="24" aria-hidden="true" />
                    <span>Take photo</span>
                </button>
                <button v-if="allowCapture" type="button" class="btn btn-tonal tile" :disabled="disabled" @click="emit('record')">
                    <Mic :size="24" aria-hidden="true" />
                    <span>Record audio</span>
                </button>
            </div>
            <p class="t-caption mt-4">PNG, JPEG, WebP, BMP, TIFF, WAV or FLAC</p>
        </template>

        <template v-else>
            <h2 id="carrier-heading" class="sr-only">Cover file</h2>
            <CarrierPreview
                v-if="status !== 'error'"
                :kind="info?.kind ?? null"
                :url="previewUrl"
                :peaks="peaks"
                :loading="status === 'loading' || previewLoading"
                :name="name"
            />
            <div class="card-body flex flex-col gap-4">
                <div class="flex items-start gap-1">
                    <div class="min-w-0 flex-1 pt-1">
                        <p class="t-section truncate">{{ name }}</p>
                    </div>
                    <DropdownMenuRoot v-model:open="menuOpen">
                        <DropdownMenuTrigger class="btn btn-ghost shrink-0 gap-1 pr-2.5" :disabled="disabled">
                            Change
                            <ChevronDown :size="16" aria-hidden="true" />
                        </DropdownMenuTrigger>
                        <DropdownMenuPortal>
                            <DropdownMenuContent class="menu" :side-offset="6" align="end">
                                <DropdownMenuItem class="menu-item" @select="emit('choose')">
                                    <FolderOpen :size="18" aria-hidden="true" />
                                    Choose file
                                </DropdownMenuItem>
                                <DropdownMenuItem v-if="allowCapture" class="menu-item" @select="emit('photo')">
                                    <Camera :size="18" aria-hidden="true" />
                                    Take photo
                                </DropdownMenuItem>
                                <DropdownMenuItem v-if="allowCapture" class="menu-item" @select="emit('record')">
                                    <Mic :size="18" aria-hidden="true" />
                                    Record audio
                                </DropdownMenuItem>
                            </DropdownMenuContent>
                        </DropdownMenuPortal>
                    </DropdownMenuRoot>
                    <button
                        type="button"
                        class="btn btn-icon shrink-0"
                        :disabled="disabled"
                        aria-label="Remove cover file"
                        @click="emit('remove')"
                    >
                        <Trash2 :size="18" aria-hidden="true" />
                    </button>
                </div>

                <p v-if="status === 'ready' && info" class="t-caption -mt-3">{{ meta }}</p>
                <div v-else-if="status === 'loading'" class="skeleton -mt-1 h-3.5 w-44" aria-hidden="true"></div>

                <div v-if="status === 'error'" class="rounded-xl bg-[color-mix(in_srgb,var(--danger)_11%,transparent)] p-4" role="alert">
                    <p class="text-[15px] font-medium text-danger">{{ error }}</p>
                    <div class="mt-3 flex flex-wrap gap-2">
                        <button type="button" class="btn btn-tonal" :disabled="disabled" @click="emit('retry')">
                            <RefreshCw :size="18" aria-hidden="true" />
                            Try again
                        </button>
                        <button type="button" class="btn btn-ghost" :disabled="disabled" @click="emit('choose')">
                            Choose file
                        </button>
                    </div>
                </div>
                <template v-else-if="status === 'ready'">
                    <p v-if="holds" class="t-caption -mt-2">{{ holds }}</p>
                    <slot />
                </template>
            </div>
        </template>
    </section>
</template>
