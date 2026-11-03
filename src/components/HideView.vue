<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
    confirmOverwrite,
    errorMessage,
    estimatePayload,
    hide,
    inspectCarrier,
    pickCarrier,
    pickPayloadFiles,
    pickSavePath,
    statFiles,
    type CarrierInfo,
    type FileStat,
    type HideReport,
} from "../api";
import { fileStem, formatBytes } from "../format";
import CapacityMeter from "./CapacityMeter.vue";
import PassphraseInput from "./PassphraseInput.vue";

const busy = defineModel<boolean>("busy", { required: true });

const carrierPath = ref<string | null>(null);
const carrier = ref<CarrierInfo | null>(null);
const inspecting = ref(false);

const text = ref("");
const files = ref<FileStat[]>([]);
const addingFiles = ref(false);

const passphrase = ref("");
const confirm = ref("");
const compress = ref(true);

const payload = ref<number | null>(null);
const estimating = ref(false);

const error = ref("");
const report = ref<HideReport | null>(null);

const hasText = computed(() => text.value.trim().length > 0);
const hasPayload = computed(() => hasText.value || files.value.length > 0);
const mismatch = computed(() => confirm.value.length > 0 && confirm.value !== passphrase.value);
const tooLarge = computed(
    () => carrier.value !== null && payload.value !== null && payload.value > carrier.value.capacity,
);
const working = computed(() => busy.value || inspecting.value || addingFiles.value);

const canHide = computed(
    () =>
        !!carrier.value &&
        hasPayload.value &&
        payload.value !== null &&
        !estimating.value &&
        !tooLarge.value &&
        passphrase.value.length > 0 &&
        passphrase.value === confirm.value &&
        !working.value,
);

// ------------------------------------------------------------ carrier

let inspectToken = 0;

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

    const token = ++inspectToken;
    carrierPath.value = picked;
    carrier.value = null;
    report.value = null;
    inspecting.value = true;
    try {
        const info = await inspectCarrier(picked);
        if (token !== inspectToken) return;
        carrier.value = info;
    } catch (e) {
        if (token !== inspectToken) return;
        carrierPath.value = null;
        error.value = errorMessage(e);
    } finally {
        if (token === inspectToken) inspecting.value = false;
    }
}

// ------------------------------------------------------------ payload

async function addFiles() {
    error.value = "";
    addingFiles.value = true;
    try {
        const picked = await pickPayloadFiles();
        if (picked.length === 0) return;
        const known = new Set(files.value.map((f) => f.path));
        const fresh = picked.filter((p) => !known.has(p));
        if (fresh.length === 0) return;
        files.value = [...files.value, ...(await statFiles(fresh))];
        report.value = null;
    } catch (e) {
        error.value = errorMessage(e);
    } finally {
        addingFiles.value = false;
    }
}

function removeFile(index: number) {
    files.value = files.value.filter((_, i) => i !== index);
}

let estimateToken = 0;
let estimateTimer: ReturnType<typeof setTimeout> | undefined;

watch([text, files], () => {
    clearTimeout(estimateTimer);
    const token = ++estimateToken;
    if (!hasPayload.value) {
        payload.value = null;
        estimating.value = false;
        return;
    }
    estimating.value = true;
    estimateTimer = setTimeout(async () => {
        try {
            const n = await estimatePayload(hasText.value ? text.value : null, files.value);
            if (token !== estimateToken) return;
            payload.value = n;
        } catch (e) {
            if (token !== estimateToken) return;
            payload.value = null;
            error.value = errorMessage(e);
        } finally {
            if (token === estimateToken) estimating.value = false;
        }
    }, 300);
});

onBeforeUnmount(() => clearTimeout(estimateTimer));

// ------------------------------------------------------------ hide

async function runHide() {
    const info = carrier.value;
    const source = carrierPath.value;
    if (!info || !source || !canHide.value) return;
    error.value = "";
    report.value = null;

    busy.value = true;
    try {
        const output = await pickSavePath(`${fileStem(info.name)}.${info.extension}`, info.extension);
        if (!output) return;
        if (output === source && !(await confirmOverwrite())) return;

        report.value = await hide({
            carrier: source,
            output,
            text: hasText.value ? text.value : null,
            files: files.value.map((f) => f.path),
            passphrase: passphrase.value,
            compress: compress.value,
        });
        passphrase.value = "";
        confirm.value = "";
    } catch (e) {
        error.value = errorMessage(e);
    } finally {
        busy.value = false;
    }
}

const changedPct = computed(() => {
    const r = report.value;
    if (!r || r.elements === 0) return "0";
    const pct = (r.changed / r.elements) * 100;
    return pct < 0.01 && r.changed > 0 ? "<0.01" : pct.toFixed(2);
});
</script>

<template>
    <div class="stack">
        <div v-if="error" class="banner error" role="alert">{{ error }}</div>
        <div v-if="report" class="banner success" role="status">
            <strong>Hidden successfully.</strong>
            <ul class="report">
                <li>Data hidden: {{ formatBytes(report.embeddedBytes) }}</li>
                <li>
                    Changed elements: {{ report.changed.toLocaleString() }} of
                    {{ report.elements.toLocaleString() }} ({{ changedPct }}%)
                </li>
                <li>Output size: {{ formatBytes(report.outputSize) }}</li>
            </ul>
        </div>

        <section class="card" aria-labelledby="h-carrier">
            <h2 id="h-carrier">1. Carrier</h2>
            <p class="muted small">The image or audio file that will hold your data.</p>
            <button type="button" class="btn secondary block-mobile" :disabled="working" @click="chooseCarrier">
                {{ carrier ? "Choose a different file" : "Choose image or audio…" }}
            </button>
            <p v-if="inspecting" class="muted" role="status">Inspecting…</p>
            <dl v-if="carrier" class="info">
                <div>
                    <dt>Name</dt>
                    <dd class="wrap">{{ carrier.name }}</dd>
                </div>
                <div>
                    <dt>Format</dt>
                    <dd>{{ carrier.format }} ({{ carrier.kind }})</dd>
                </div>
                <div>
                    <dt>Details</dt>
                    <dd>{{ carrier.description }}</dd>
                </div>
                <div>
                    <dt>Size</dt>
                    <dd>{{ formatBytes(carrier.size) }}</dd>
                </div>
            </dl>
        </section>

        <section class="card" aria-labelledby="h-payload">
            <h2 id="h-payload">2. What to hide</h2>
            <div class="field">
                <label for="hide-text">Text message (optional)</label>
                <textarea
                    id="hide-text"
                    v-model="text"
                    rows="5"
                    :disabled="busy"
                    placeholder="Type a secret message…"
                ></textarea>
            </div>

            <div class="field">
                <span class="label">Files (optional)</span>
                <ul v-if="files.length" class="list">
                    <li v-for="(f, i) in files" :key="f.path" class="list-item">
                        <div class="grow">
                            <div class="wrap">{{ f.name }}</div>
                            <div class="muted small">{{ formatBytes(f.size) }}</div>
                        </div>
                        <button
                            type="button"
                            class="btn ghost"
                            :disabled="busy"
                            :aria-label="`Remove ${f.name}`"
                            @click="removeFile(i)"
                        >
                            Remove
                        </button>
                    </li>
                </ul>
                <button type="button" class="btn secondary block-mobile" :disabled="working" @click="addFiles">
                    Add files…
                </button>
            </div>

            <p v-if="!hasPayload" class="muted small">Enter a message or add at least one file.</p>
        </section>

        <section v-if="carrier" class="card" aria-labelledby="h-cap">
            <h2 id="h-cap">Capacity</h2>
            <CapacityMeter
                :payload="hasPayload ? payload : 0"
                :capacity="carrier.capacity"
                :recommended="carrier.recommended"
                :estimating="estimating"
            />
        </section>

        <section class="card" aria-labelledby="h-secure">
            <h2 id="h-secure">3. Protect</h2>
            <PassphraseInput
                id="hide-pass"
                v-model="passphrase"
                label="Passphrase"
                show-strength
                autocomplete="new-password"
                :disabled="busy"
            />
            <PassphraseInput
                id="hide-confirm"
                v-model="confirm"
                label="Confirm passphrase"
                autocomplete="new-password"
                :disabled="busy"
            />
            <p v-if="mismatch" class="field-error" role="alert">Passphrases do not match.</p>

            <label class="check">
                <input v-model="compress" type="checkbox" :disabled="busy" />
                <span>Compress payload</span>
            </label>
        </section>

        <button type="button" class="btn primary" :disabled="!canHide" @click="runHide">
            {{ busy ? "Hiding…" : "Hide and save…" }}
        </button>
    </div>
</template>
