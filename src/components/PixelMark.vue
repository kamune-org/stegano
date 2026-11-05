<script setup lang="ts">
// The logo's pixel grid, drawn with theme colours: one mint pixel.
const opacities = [0.35, 0.6, 0.45, 0.8, 0.7, 0.4, 0.9, 0.5, 0.5, 0.85, 1, 0.35, 0.9, 0.45, 0.65, 0.4];
</script>

<template>
    <div class="pixel-mark" aria-hidden="true">
        <svg viewBox="0 0 44 44" width="44" height="44">
            <rect
                v-for="(opacity, index) in opacities"
                :key="index"
                :x="(index % 4) * 11.5"
                :y="Math.floor(index / 4) * 11.5"
                width="9"
                height="9"
                rx="2.4"
                :fill="index === 10 ? 'var(--mint)' : 'var(--primary)'"
                :fill-opacity="index === 10 ? 1 : opacity"
            />
        </svg>
    </div>
</template>
