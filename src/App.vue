<script setup lang="ts">
import { ref } from "vue";
import HideView from "./components/HideView.vue";
import RevealView from "./components/RevealView.vue";

type Tab = "hide" | "reveal";

const tab = ref<Tab>("hide");
const busy = ref(false);

function select(next: Tab) {
    if (!busy.value) tab.value = next;
}
</script>

<template>
    <div class="container">
        <header class="header">
            <h1>Stegano</h1>
            <p class="subtitle">Hide encrypted messages and files in images and audio</p>
        </header>

        <div class="tabs" role="tablist" aria-label="Mode">
            <button
                id="tab-hide"
                type="button"
                role="tab"
                class="tab"
                :class="{ active: tab === 'hide' }"
                :aria-selected="tab === 'hide'"
                aria-controls="panel-hide"
                :disabled="busy && tab !== 'hide'"
                @click="select('hide')"
            >
                Hide
            </button>
            <button
                id="tab-reveal"
                type="button"
                role="tab"
                class="tab"
                :class="{ active: tab === 'reveal' }"
                :aria-selected="tab === 'reveal'"
                aria-controls="panel-reveal"
                :disabled="busy && tab !== 'reveal'"
                @click="select('reveal')"
            >
                Reveal
            </button>
        </div>

        <main>
            <div v-show="tab === 'hide'" id="panel-hide" role="tabpanel" aria-labelledby="tab-hide">
                <HideView v-model:busy="busy" />
            </div>
            <div v-show="tab === 'reveal'" id="panel-reveal" role="tabpanel" aria-labelledby="tab-reveal">
                <RevealView v-model:busy="busy" :active="tab === 'reveal'" />
            </div>
        </main>

        <footer class="footer">
            The output file has the same format as the input. Sending it through apps that recompress media
            (messengers, social networks) destroys the hidden data. Share it as a file or document instead.
        </footer>
    </div>
</template>
