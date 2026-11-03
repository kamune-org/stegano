<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
    clearRevealed,
    errorMessage,
    inspectCarrier,
    pickCarrier,
    pickSavePath,
    reveal,
    saveEntry,
    type RevealedEntry,
} from "../api";
import { displayName, formatBytes, formatDate, safeFileName } from "../format";
import PassphraseInput from "./PassphraseInput.vue";

const props = defineProps<{ active: boolean }>();
const busy = defineModel<boolean>("busy", { required: true });

const carrierPath = ref<string | null>(null);
const carrierName = ref("");
const passphrase = ref("");
const entries = ref<RevealedEntry[]>([]);
const revealedOnce = ref(false);

const error = ref("");
const notice = ref("");
const copied = ref<number | null>(null);
const saved = ref<Set<number>>(new Set());

const canReveal = computed(() => !!carrierPath.value && passphrase.value.length > 0 && !busy.value);

function wipe() {
    entries.value = [];
    revealedOnce.value = false;
    copied.value = null;
    saved.value = new Set();
    error.value = "";
    notice.value = "";
    clearRevealed().catch(() => {});
}

async function chooseCarrier() {
    error.value = "";
    let picked: string | null;
    try {
        picked = await pickCarrier();
    } catch (e) {
        error.value = errorMessage(e);
        return;
    }
    if (!picked) return;
    wipe();
    carrierPath.value = picked;
    carrierName.value = displayName(picked);
    // The real name is nicer than a URI tail, but not essential.
    inspectCarrier(picked)
        .then((info) => {
            if (carrierPath.value === picked) carrierName.value = info.name;
        })
        .catch(() => {});
}

async function runReveal() {
    if (!carrierPath.value || !canReveal.value) return;
    error.value = "";
    notice.value = "";
    entries.value = [];
    saved.value = new Set();
    revealedOnce.value = false;
    busy.value = true;
    try {
        entries.value = await reveal(carrierPath.value, passphrase.value);
        revealedOnce.value = true;
        passphrase.value = "";
    } catch (e) {
        error.value = errorMessage(e);
    } finally {
        busy.value = false;
    }
}

async function copyText(entry: RevealedEntry) {
    if (entry.text === null) return;
    error.value = "";
    try {
        await navigator.clipboard.writeText(entry.text);
        copied.value = entry.index;
        setTimeout(() => {
            if (copied.value === entry.index) copied.value = null;
        }, 2000);
    } catch {
        error.value = "Could not copy to the clipboard. Select the text and copy it manually.";
    }
}

async function saveFile(entry: RevealedEntry) {
    error.value = "";
    notice.value = "";
    busy.value = true;
    try {
        const path = await pickSavePath(safeFileName(entry.name));
        if (!path) return;
        await saveEntry(entry.index, path);
        saved.value = new Set(saved.value).add(entry.index);
        notice.value = `Saved ${safeFileName(entry.name)}.`;
    } catch (e) {
        error.value = errorMessage(e);
    } finally {
        busy.value = false;
    }
}

function clearAll() {
    wipe();
    passphrase.value = "";
}

watch(
    () => props.active,
    (active) => {
        if (!active && !busy.value) {
            wipe();
            passphrase.value = "";
        }
    },
);

onBeforeUnmount(() => {
    clearRevealed().catch(() => {});
});
</script>

<template>
    <div class="stack">
        <div v-if="error" class="banner error" role="alert">{{ error }}</div>
        <div v-if="notice" class="banner success" role="status">{{ notice }}</div>

        <section class="card" aria-labelledby="r-carrier">
            <h2 id="r-carrier">1. Carrier</h2>
            <p class="muted small">The image or audio file that contains hidden data.</p>
            <button type="button" class="btn secondary block-mobile" :disabled="busy" @click="chooseCarrier">
                {{ carrierPath ? "Choose a different file" : "Choose image or audio…" }}
            </button>
            <p v-if="carrierPath" class="selected wrap">{{ carrierName }}</p>
        </section>

        <section class="card" aria-labelledby="r-pass">
            <h2 id="r-pass">2. Passphrase</h2>
            <PassphraseInput
                id="reveal-pass"
                v-model="passphrase"
                label="Passphrase"
                autocomplete="current-password"
                :disabled="busy"
            />
        </section>

        <button type="button" class="btn primary" :disabled="!canReveal" @click="runReveal">
            {{ busy ? "Working…" : "Reveal" }}
        </button>

        <section v-if="revealedOnce" class="card" aria-labelledby="r-result">
            <div class="row-between">
                <h2 id="r-result">Hidden content</h2>
                <button type="button" class="btn ghost" :disabled="busy" @click="clearAll">Clear</button>
            </div>
            <p v-if="entries.length === 0" class="muted">The hidden payload is empty.</p>

            <ul class="entries">
                <li v-for="entry in entries" :key="entry.index" class="entry">
                    <template v-if="entry.kind === 'text'">
                        <label :for="`entry-${entry.index}`" class="label">Message</label>
                        <textarea
                            :id="`entry-${entry.index}`"
                            class="mono"
                            readonly
                            rows="6"
                            :value="entry.text ?? ''"
                        ></textarea>
                        <button type="button" class="btn secondary block-mobile" @click="copyText(entry)">
                            {{ copied === entry.index ? "Copied" : "Copy" }}
                        </button>
                    </template>
                    <template v-else>
                        <div class="list-item">
                            <div class="grow">
                                <div class="wrap">{{ entry.name }}</div>
                                <div class="muted small">
                                    {{ formatBytes(entry.size) }}
                                    <template v-if="entry.modified !== null">
                                        · modified {{ formatDate(entry.modified) }}
                                    </template>
                                </div>
                            </div>
                            <button
                                type="button"
                                class="btn secondary"
                                :disabled="busy"
                                :aria-label="`Save ${entry.name}`"
                                @click="saveFile(entry)"
                            >
                                {{ saved.has(entry.index) ? "Saved" : "Save…" }}
                            </button>
                        </div>
                    </template>
                </li>
            </ul>
        </section>
    </div>
</template>
