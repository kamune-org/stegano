<script setup lang="ts">
import { Copy, Dices } from "@lucide/vue";
import { toast } from "vue-sonner";
import { generatePassphrase } from "../lib/passphrase";

const props = defineProps<{ passphrase: string; disabled?: boolean }>();
const emit = defineEmits<{ generate: [value: string] }>();

function generate(): void {
    emit("generate", generatePassphrase());
}

async function copy(): Promise<void> {
    if (!props.passphrase) return;
    try {
        await navigator.clipboard.writeText(props.passphrase);
        toast.success("Passphrase copied.");
    } catch {
        toast.error("Could not copy the passphrase.");
    }
}
</script>

<template>
    <div class="flex flex-wrap items-center gap-2">
        <button type="button" class="btn btn-tonal" :disabled="disabled" @click="generate">
            <Dices :size="18" aria-hidden="true" />
            Generate
        </button>
        <button type="button" class="btn btn-ghost" :disabled="disabled || !passphrase" @click="copy">
            <Copy :size="18" aria-hidden="true" />
            Copy
        </button>
    </div>
</template>
