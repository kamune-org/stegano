<script setup lang="ts">
import { computed, ref } from "vue";

const props = defineProps<{
    id: string;
    label: string;
    showStrength?: boolean;
    disabled?: boolean;
    autocomplete?: string;
}>();

const model = defineModel<string>({ required: true });
const visible = ref(false);

const strength = computed(() => {
    const p = model.value;
    if (!p) return { level: 0, label: "", cls: "" };

    let score = 0;
    if (p.length >= 6) score++;
    if (p.length >= 10) score++;
    if (p.length >= 14) score++;
    if (/[a-z]/.test(p) && /[A-Z]/.test(p)) score++;
    if (/\d/.test(p)) score++;
    if (/[^a-zA-Z0-9]/.test(p)) score++;

    if (score <= 2) return { level: score, label: "Weak", cls: "weak" };
    if (score <= 4) return { level: score, label: "Medium", cls: "medium" };
    return { level: score, label: "Strong", cls: "strong" };
});

const hintId = computed(() => `${props.id}-strength`);
</script>

<template>
    <div class="field">
        <label :for="id">{{ label }}</label>
        <div class="pass-row">
            <input
                :id="id"
                v-model="model"
                :type="visible ? 'text' : 'password'"
                :disabled="disabled"
                :autocomplete="autocomplete ?? 'off'"
                :aria-describedby="showStrength ? hintId : undefined"
                autocapitalize="off"
                autocorrect="off"
                spellcheck="false"
            />
            <button
                type="button"
                class="btn ghost pass-toggle"
                :aria-pressed="visible"
                :aria-label="visible ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`"
                @click="visible = !visible"
            >
                {{ visible ? "Hide" : "Show" }}
            </button>
        </div>
        <div v-if="showStrength" :id="hintId" class="strength" aria-live="polite">
            <div class="strength-bar" aria-hidden="true">
                <div
                    class="strength-fill"
                    :class="strength.cls"
                    :style="{ width: (strength.level / 6) * 100 + '%' }"
                ></div>
            </div>
            <span class="strength-label" :class="strength.cls">
                {{ strength.label ? `Strength: ${strength.label}` : "" }}
            </span>
        </div>
    </div>
</template>
