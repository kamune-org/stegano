<script setup lang="ts">
import { Check } from "@lucide/vue";

export type Step = { label: string; done: boolean };

defineProps<{ steps: Step[] }>();
</script>

<template>
    <ol class="steps" aria-label="Progress">
        <template v-for="(step, index) in steps" :key="step.label">
            <li
                v-if="index > 0"
                class="step-line"
                :data-done="steps[index - 1]?.done ? 'true' : 'false'"
                role="presentation"
                aria-hidden="true"
            ></li>
            <li
                class="step"
                :data-state="step.done ? 'done' : steps.findIndex((item) => !item.done) === index ? 'current' : 'todo'"
                :aria-label="`${step.label}${step.done ? ', done' : ''}`"
            >
                <span class="step-dot" aria-hidden="true">
                    <Check v-if="step.done" :size="14" :stroke-width="3" class="step-check" />
                    <template v-else>{{ index + 1 }}</template>
                </span>
                <span aria-hidden="true">{{ step.label }}</span>
            </li>
        </template>
    </ol>
</template>
