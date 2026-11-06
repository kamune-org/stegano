<script setup lang="ts">
defineProps<{ title: string; scrolled?: boolean }>();
</script>

<template>
    <header class="app-bar" :data-scrolled="scrolled ? 'true' : 'false'">
        <h1 class="t-screen">{{ title }}</h1>
    </header>
</template>
