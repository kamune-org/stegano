<script setup lang="ts">
import { computed, ref } from "vue";
import { Camera, ChevronDown, FolderOpen, ImagePlus, Mic, RefreshCw, Trash2 } from "@lucide/vue";
import { isAndroid } from "../api/pickers";
import { useCoarsePointer } from "../lib/keyboard";
import { useLayout, withShortcut } from "../lib/layout";
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
    /** Desktop info bar: "holds 879 KiB, safe 220 KiB". */
    holdsShort?: string;
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
const { desktop } = useLayout();
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

const metaDesktop = computed(() => [meta.value, props.holdsShort].filter(Boolean).join(" · "));
const chooseTip = computed(() => withShortcut("Choose cover file", "O"));
</script>

<template>
    <section
        ref="root"
        class="card"
        :class="[status === 'empty' ? 'dropzone' : 'card-flush', { hot, cover: desktop }]"
        aria-labelledby="carrier-heading"
        :aria-busy="status === 'loading'"
        @dragover="onDragOver"
        @drop="onDrop"
    >
        <template v-if="status === 'empty'">
            <div v-if="desktop" class="cover-empty">
                <ImagePlus :size="34" :stroke-width="1.5" class="cover-empty-icon" aria-hidden="true" />
                <h2 id="carrier-heading" class="cover-title">Drop an image or audio file</h2>
                <p class="t-caption">or</p>
                <div class="cover-buttons">
                    <button type="button" class="btn btn-tonal" :disabled="disabled" :title="chooseTip" @click="emit('choose')">
                        <FolderOpen :size="14" aria-hidden="true" />
                        Choose File…
                    </button>
                    <button v-if="allowCapture" type="button" class="btn btn-tonal" :disabled="disabled" @click="emit('photo')">
                        <Camera :size="14" aria-hidden="true" />
                        Take Photo…
                    </button>
                    <button v-if="allowCapture" type="button" class="btn btn-tonal" :disabled="disabled" @click="emit('record')">
                        <Mic :size="14" aria-hidden="true" />
                        Record Audio…
                    </button>
                </div>
                <p class="t-caption cover-formats">PNG, JPEG, WebP, BMP, TIFF, WAV or FLAC</p>
            </div>
            <template v-else>
                <div class="empty-icon" aria-hidden="true">
                    <ImagePlus :size="44" :stroke-width="1.5" />
                </div>
                <h2 id="carrier-heading" class="t-section mt-3">
                    {{ dropAvailable ? "Drop a cover file here" : "Choose a cover file" }}
                </h2>
                <p class="mt-1 max-w-[30ch] t-body text-muted">The photo or audio your secret hides inside.</p>
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
        </template>

        <template v-else-if="desktop">
            <h2 id="carrier-heading" class="sr-only">Cover file</h2>
            <div class="cover-well">
                <CarrierPreview
                    v-if="status !== 'error'"
                    :kind="info?.kind ?? null"
                    :url="previewUrl"
                    :peaks="peaks"
                    :loading="status === 'loading' || previewLoading"
                    :name="name"
                />
                <div v-else class="cover-error" role="alert">
                    <p class="t-body font-medium text-danger">{{ error }}</p>
                    <div class="cover-buttons">
                        <button type="button" class="btn btn-tonal" :disabled="disabled" @click="emit('retry')">
                            <RefreshCw :size="14" aria-hidden="true" />
                            Try Again
                        </button>
                        <button type="button" class="btn btn-ghost" :disabled="disabled" :title="chooseTip" @click="emit('choose')">
                            Choose File…
                        </button>
                    </div>
                </div>
            </div>
            <div class="infobar">
                <div class="infobar-row">
                    <p class="infobar-name" :title="name">{{ name }}</p>
                    <div class="infobar-actions">
                        <DropdownMenuRoot v-if="allowCapture" v-model:open="menuOpen">
                            <DropdownMenuTrigger class="btn btn-tonal btn-sm" :disabled="disabled">
                                Change…
                                <ChevronDown :size="12" aria-hidden="true" />
                            </DropdownMenuTrigger>
                            <DropdownMenuPortal>
                                <DropdownMenuContent class="menu" :side-offset="4" align="end">
                                    <DropdownMenuItem class="menu-item" @select="emit('choose')">
                                        <FolderOpen :size="14" aria-hidden="true" />
                                        Choose File…
                                    </DropdownMenuItem>
                                    <DropdownMenuItem class="menu-item" @select="emit('photo')">
                                        <Camera :size="14" aria-hidden="true" />
                                        Take Photo…
                                    </DropdownMenuItem>
                                    <DropdownMenuItem class="menu-item" @select="emit('record')">
                                        <Mic :size="14" aria-hidden="true" />
                                        Record Audio…
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenuPortal>
                        </DropdownMenuRoot>
                        <button v-else type="button" class="btn btn-tonal btn-sm" :disabled="disabled" :title="chooseTip" @click="emit('choose')">
                            Change…
                        </button>
                        <button type="button" class="btn btn-tonal btn-sm" :disabled="disabled" aria-label="Remove cover file" title="Remove cover file" @click="emit('remove')">
                            <Trash2 :size="13" aria-hidden="true" />
                            Remove
                        </button>
                    </div>
                </div>
                <p v-if="status === 'ready' && info" class="infobar-meta">{{ metaDesktop }}</p>
                <div v-else-if="status === 'loading'" class="skeleton infobar-skeleton" aria-hidden="true"></div>
                <slot v-if="status === 'ready'" />
            </div>
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
                    <p class="t-body font-medium text-danger">{{ error }}</p>
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
