<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { SwitchCamera, X } from "@lucide/vue";
import { stampedName } from "../lib/format";
import ResponsiveSheet from "./ResponsiveSheet.vue";

const open = defineModel<boolean>("open", { required: true });
const emit = defineEmits<{ use: [name: string, bytes: Uint8Array] }>();

const videoEl = ref<HTMLVideoElement | null>(null);
const phase = ref<"starting" | "live" | "review" | "error">("starting");
const error = ref("");
const shotUrl = ref<string | null>(null);
const cameras = ref(0);
const facing = ref<"environment" | "user">("environment");

let session = 0;
let stream: MediaStream | null = null;
let shot: Blob | null = null;

function stopTracks(current: MediaStream | null): void {
    current?.getTracks().forEach((track) => track.stop());
}

function revokeShot(): void {
    if (shotUrl.value) URL.revokeObjectURL(shotUrl.value);
    shotUrl.value = null;
    shot = null;
}

function stopAll(): void {
    session++;
    stopTracks(stream);
    stream = null;
    revokeShot();
    phase.value = "starting";
    error.value = "";
}

function messageFor(value: unknown): string {
    const name = value instanceof DOMException || value instanceof Error ? value.name : "";
    if (name === "NotAllowedError" || name === "PermissionDeniedError" || name === "SecurityError") {
        return "Camera access was denied. Allow the camera in system settings, then try again.";
    }
    if (name === "NotFoundError" || name === "DevicesNotFoundError") return "No camera was found on this device.";
    if (name === "NotReadableError" || name === "TrackStartError") return "The camera is in use by another app.";
    return "The camera could not be started.";
}

function isOverconstrained(value: unknown): boolean {
    return (value instanceof DOMException || value instanceof Error) && value.name === "OverconstrainedError";
}

function attach(next: MediaStream): void {
    stream = next;
    const video = videoEl.value;
    if (!video) return;
    video.srcObject = next;
    void video.play().catch(() => {});
}

async function start(): Promise<void> {
    const mine = ++session;
    stopTracks(stream);
    stream = null;
    error.value = "";
    phase.value = "starting";
    const ideal = {
        video: { facingMode: facing.value, width: { ideal: 4096 }, height: { ideal: 3072 } },
        audio: false,
    };
    let next: MediaStream;
    try {
        next = await navigator.mediaDevices.getUserMedia(ideal);
    } catch (cause) {
        if (!isOverconstrained(cause)) {
            if (mine !== session || !open.value) return;
            phase.value = "error";
            error.value = messageFor(cause);
            return;
        }
        try {
            next = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        } catch (fallback) {
            if (mine !== session || !open.value) return;
            phase.value = "error";
            error.value = messageFor(fallback);
            return;
        }
    }
    if (mine !== session || !open.value) {
        stopTracks(next);
        return;
    }
    attach(next);
    phase.value = "live";
    try {
        const devices = await navigator.mediaDevices.enumerateDevices();
        if (mine === session) cameras.value = devices.filter((device) => device.kind === "videoinput").length;
    } catch {
        cameras.value = 0;
    }
}

watch(videoEl, (video) => {
    if (video && stream && open.value) {
        video.srcObject = stream;
        void video.play().catch(() => {});
    }
});

watch(open, (isOpen) => {
    if (isOpen) void start();
    else stopAll();
});

onBeforeUnmount(stopAll);

async function switchCamera(): Promise<void> {
    facing.value = facing.value === "environment" ? "user" : "environment";
    stopTracks(stream);
    stream = null;
    await start();
}

async function capture(): Promise<void> {
    const video = videoEl.value;
    if (!video || video.videoWidth === 0 || video.videoHeight === 0) return;
    const canvas = document.createElement("canvas");
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.drawImage(video, 0, 0);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!open.value) return;
    if (!blob) {
        phase.value = "error";
        error.value = "Could not encode the photo.";
        return;
    }
    revokeShot();
    shot = blob;
    shotUrl.value = URL.createObjectURL(blob);
    phase.value = "review";
}

function retake(): void {
    revokeShot();
    phase.value = stream ? "live" : "starting";
    if (!stream) void start();
}

async function usePhoto(): Promise<void> {
    if (!shot) return;
    const bytes = new Uint8Array(await shot.arrayBuffer());
    emit("use", stampedName("photo", "png"), bytes);
    open.value = false;
}
</script>

<template>
    <ResponsiveSheet
        :open="open"
        bare
        fullscreen
        title="Take a photo"
        description="The photo becomes the carrier."
        @close="open = false"
    >
        <div class="flex min-h-[100dvh] flex-col bg-black text-white">
            <div class="flex items-center justify-between px-4 pt-[max(0.75rem,env(safe-area-inset-top))]">
                <button type="button" class="btn btn-icon text-white" aria-label="Close camera" @click="open = false">
                    <X :size="22" aria-hidden="true" />
                </button>
                <button
                    v-if="cameras > 1 && phase !== 'review'"
                    type="button"
                    class="btn btn-icon text-white"
                    aria-label="Switch camera"
                    @click="switchCamera"
                >
                    <SwitchCamera :size="22" aria-hidden="true" />
                </button>
            </div>

            <div class="relative min-h-0 flex-1">
                <video
                    v-show="phase !== 'review'"
                    ref="videoEl"
                    class="h-full w-full object-cover"
                    :class="facing === 'user' ? '-scale-x-100' : ''"
                    autoplay
                    muted
                    playsinline
                ></video>
                <img v-if="phase === 'review' && shotUrl" :src="shotUrl" alt="Captured photo" class="h-full w-full object-contain" />
                <p v-if="phase === 'error'" class="absolute inset-0 grid place-items-center px-8 text-center text-base" role="alert">
                    {{ error }}
                </p>
            </div>

            <div class="flex items-center justify-center gap-4 px-4 pt-4 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
                <template v-if="phase === 'review'">
                    <button type="button" class="btn btn-tonal" @click="retake">Retake</button>
                    <button type="button" class="btn btn-primary" @click="usePhoto">Use photo</button>
                </template>
                <button
                    v-else-if="phase === 'error'"
                    type="button"
                    class="btn btn-primary"
                    @click="start"
                >
                    Try again
                </button>
                <button
                    v-else
                    type="button"
                    class="shutter"
                    :disabled="phase !== 'live'"
                    aria-label="Take photo"
                    @click="capture"
                ></button>
            </div>
        </div>
    </ResponsiveSheet>
</template>
