<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { formatDuration, stampedName } from "../lib/format";
import { blobBytes } from "../lib/preview";
import { encodeWav } from "../lib/wav";
import ResponsiveSheet from "./ResponsiveSheet.vue";

const open = defineModel<boolean>("open", { required: true });
const emit = defineEmits<{ use: [name: string, bytes: Uint8Array] }>();

const phase = ref<"idle" | "recording" | "preview" | "error">("idle");
const error = ref("");
const elapsedMs = ref(0);
const level = ref(0);
const previewUrl = ref<string | null>(null);

const MAX_SAMPLES_SECONDS = 600;
let context: AudioContext | null = null;
let stream: MediaStream | null = null;
let chunks: Float32Array[] = [];
let samples = 0;
let wavBytes: Uint8Array | null = null;

function rms(data: Float32Array): number {
    if (data.length === 0) return 0;
    let sum = 0;
    for (let i = 0; i < data.length; i++) sum += (data[i] ?? 0) ** 2;
    return Math.sqrt(sum / data.length);
}

function messageFor(value: unknown): string {
    const name = value instanceof DOMException || value instanceof Error ? value.name : "";
    if (name === "NotAllowedError" || name === "PermissionDeniedError" || name === "SecurityError") {
        return "Microphone access was denied. Allow the microphone in system settings, then try again.";
    }
    if (name === "NotFoundError" || name === "DevicesNotFoundError") return "No microphone was found on this device.";
    if (name === "NotReadableError" || name === "TrackStartError") return "The microphone is in use by another app.";
    return "The microphone could not be started.";
}

function releaseGraph(): void {
    stream?.getTracks().forEach((track) => track.stop());
    stream = null;
    const current = context;
    context = null;
    if (current) void current.close().catch(() => {});
}

function revokePreview(): void {
    if (previewUrl.value) URL.revokeObjectURL(previewUrl.value);
    previewUrl.value = null;
}

function resetRecording(): void {
    releaseGraph();
    revokePreview();
    chunks = [];
    samples = 0;
    wavBytes = null;
    elapsedMs.value = 0;
    level.value = 0;
}

function closeAll(): void {
    resetRecording();
    phase.value = "idle";
    error.value = "";
}

async function stopRecording(): Promise<void> {
    if (phase.value !== "recording") return;
    phase.value = "preview";
    const rate = context?.sampleRate || 48000;
    const taken = chunks;
    chunks = [];
    releaseGraph();
    if (taken.length === 0 || samples === 0) {
        phase.value = "error";
        error.value = "The recording is empty.";
        return;
    }
    wavBytes = encodeWav(taken, rate);
    revokePreview();
    previewUrl.value = URL.createObjectURL(new Blob([blobBytes(wavBytes)], { type: "audio/wav" }));
    phase.value = "preview";
    level.value = 0;
}

async function record(): Promise<void> {
    error.value = "";
    resetRecording();
    const audio = new AudioContext();
    context = audio;
    if (audio.state === "suspended") {
        try {
            await audio.resume();
        } catch {
            error.value = "The audio engine did not start. Try again.";
            phase.value = "error";
            releaseGraph();
            return;
        }
    }
    let mic: MediaStream;
    try {
        mic = await navigator.mediaDevices.getUserMedia({
            audio: {
                echoCancellation: false,
                noiseSuppression: false,
                autoGainControl: false,
                channelCount: 1,
            },
        });
    } catch (cause) {
        if (!open.value) {
            releaseGraph();
            return;
        }
        phase.value = "error";
        error.value = messageFor(cause);
        releaseGraph();
        return;
    }
    if (!open.value) {
        mic.getTracks().forEach((track) => track.stop());
        releaseGraph();
        return;
    }
    try {
        await audio.audioWorklet.addModule("/recorder-worklet.js");
    } catch {
        mic.getTracks().forEach((track) => track.stop());
        phase.value = "error";
        error.value = "Recording is not available in this browser.";
        releaseGraph();
        return;
    }
    stream = mic;
    const source = audio.createMediaStreamSource(mic);
    const node = new AudioWorkletNode(audio, "recorder-processor");
    const silent = audio.createGain();
    silent.gain.value = 0;
    const rate = audio.sampleRate;
    const maxSamples = rate * MAX_SAMPLES_SECONDS;
    node.port.onmessage = (event: MessageEvent) => {
        const data = event.data;
        if (!(data instanceof Float32Array) || phase.value !== "recording") return;
        if (samples >= maxSamples) return;
        const room = maxSamples - samples;
        const piece = data.length > room ? new Float32Array(data.subarray(0, room)) : data;
        chunks.push(piece);
        samples += piece.length;
        level.value = Math.min(1, rms(piece) * 3);
        elapsedMs.value = (samples / rate) * 1000;
        if (samples >= maxSamples) void stopRecording();
    };
    source.connect(node);
    node.connect(silent);
    silent.connect(audio.destination);
    phase.value = "recording";
}

function useRecording(): void {
    if (!wavBytes) return;
    emit("use", stampedName("recording", "wav"), wavBytes);
    open.value = false;
}

watch(open, (isOpen) => {
    if (!isOpen) closeAll();
});

onBeforeUnmount(closeAll);
</script>

<template>
    <ResponsiveSheet
        :open="open"
        title="Record audio"
        description="Natural noise hides the data. Processing filters stay off."
        @close="open = false"
    >
        <div class="flex flex-col items-center gap-4 py-2">
            <p class="text-4xl font-semibold tabular-nums" aria-live="off">{{ formatDuration(elapsedMs) }}</p>
            <p class="text-xs text-muted">10:00 maximum</p>
            <div class="level w-full" aria-hidden="true">
                <span :style="{ width: `${Math.round(level * 100)}%` }"></span>
            </div>
            <p v-if="phase === 'error'" class="text-center text-sm text-danger" role="alert">{{ error }}</p>
            <audio v-if="phase === 'preview' && previewUrl" :src="previewUrl" controls class="w-full"></audio>
            <div class="flex w-full flex-col gap-2">
                <button
                    v-if="phase !== 'recording' && phase !== 'preview'"
                    type="button"
                    class="btn btn-primary w-full"
                    @click="record"
                >
                    Record
                </button>
                <button v-if="phase === 'recording'" type="button" class="btn btn-primary w-full" @click="stopRecording">
                    Stop
                </button>
                <template v-if="phase === 'preview'">
                    <button type="button" class="btn btn-tonal w-full" @click="record">Re-record</button>
                    <button type="button" class="btn btn-primary w-full" @click="useRecording">Use recording</button>
                </template>
            </div>
        </div>
    </ResponsiveSheet>
</template>
