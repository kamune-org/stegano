<script setup lang="ts">
import {
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from "reka-ui";
import { useBackstack } from "../lib/backstack";

const open = defineModel<boolean>("open", { required: true });
const props = withDefaults(
    defineProps<{
        title: string;
        description?: string;
        dismissable?: boolean;
        bare?: boolean;
        fullscreen?: boolean;
    }>(),
    { description: "", dismissable: true, bare: false, fullscreen: false },
);
const emit = defineEmits<{ close: []; blocked: [] }>();

function requestClose(): void {
    if (!props.dismissable) emit("blocked");
    else emit("close");
}

useBackstack(open, requestClose, { persist: props.dismissable === false });

function onUpdate(next: boolean): void {
    if (!next) requestClose();
}

function onDismiss(event: Event): void {
    event.preventDefault();
    requestClose();
}
</script>

<template>
    <DialogRoot :open="open" @update:open="onUpdate">
        <DialogPortal>
            <DialogOverlay class="overlay" />
            <DialogContent
                class="sheet-panel"
                :class="{ 'sheet-fullscreen': fullscreen }"
                @escape-key-down="onDismiss"
                @pointer-down-outside="onDismiss"
                @interact-outside="onDismiss"
                @focus-outside="onDismiss"
            >
                <template v-if="bare">
                    <DialogTitle class="sr-only">{{ title }}</DialogTitle>
                    <DialogDescription class="sr-only">{{ description || title }}</DialogDescription>
                </template>
                <template v-else>
                    <div class="grabber md:hidden" aria-hidden="true"></div>
                    <DialogTitle class="sheet-title">{{ title }}</DialogTitle>
                    <DialogDescription v-if="description" class="sheet-desc">{{ description }}</DialogDescription>
                    <DialogDescription v-else class="sr-only">{{ title }}</DialogDescription>
                </template>
                <div :class="bare ? 'sheet-body' : 'sheet-body mt-4'">
                    <slot />
                </div>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>
