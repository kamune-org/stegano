<script setup lang="ts">
import logoUrl from "../assets/logo.svg";

defineProps<{ title: string; scrolled?: boolean }>();
</script>

<template>
    <header class="app-bar md:hidden" :data-scrolled="scrolled ? 'true' : 'false'">
        <img :src="logoUrl" alt="" aria-hidden="true" class="size-7 shrink-0 rounded-[8px]" />
        <h1 class="t-screen">{{ title }}</h1>
    </header>
</template>
