<script setup lang="ts">
import { computed } from "vue";
import { Eye, EyeOff } from "@lucide/vue";
import { passphraseStrength } from "../lib/passphrase";

const model = defineModel<string>({ required: true });
const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{
    id: string;
    label: string;
    showStrength?: boolean;
    disabled?: boolean;
    error?: string;
    hideLabel?: boolean;
}>();
const emit = defineEmits<{ enter: [] }>();

const strength = computed(() => passphraseStrength(props.showStrength ? model.value : ""));
const segments = computed(() => {
    const { tone, level } = strength.value;
    if (tone === "weak") return 1;
    if (tone === "medium") return level - 1;
    if (tone === "strong") return 4;
    return 0;
});
const hintId = computed(() => `${props.id}-hint`);
</script>

<template>
    <div>
        <label :for="id" :class="hideLabel ? 'sr-only' : 't-label mb-2 block'">{{ label }}</label>
        <div class="field-wrap">
            <input
                :id="id"
                v-model="model"
                class="field mono"
                :type="visible ? 'text' : 'password'"
                :disabled="disabled"
                autocomplete="off"
                autocapitalize="off"
                autocorrect="off"
                spellcheck="false"
                :aria-describedby="hintId"
                :aria-invalid="error ? true : undefined"
                @keydown.enter.prevent="emit('enter')"
            />
            <button
                type="button"
                class="btn btn-icon field-toggle"
                :aria-pressed="visible"
                :aria-label="visible ? `Hide ${label}` : `Show ${label}`"
                :disabled="disabled"
                @click="visible = !visible"
            >
                <EyeOff v-if="visible" :size="18" aria-hidden="true" />
                <Eye v-else :size="18" aria-hidden="true" />
            </button>
        </div>
        <div v-if="showStrength" class="strength-row mt-3 flex items-center gap-3" aria-live="polite">
            <div class="strength-bars" :data-tone="strength.tone" aria-hidden="true">
                <span v-for="n in 4" :key="n" :class="{ on: n <= segments }"></span>
            </div>
            <span class="strength-label w-14 text-right text-[13px] font-medium" :class="strength.tone === 'none' ? '' : `tone-${strength.tone}`">
                {{ strength.label }}
            </span>
        </div>
        <p :id="hintId" class="pass-hint text-[13px] text-danger" :class="error ? 'mt-2' : ''" role="alert" aria-live="polite">
            {{ error }}
        </p>
    </div>
</template>
