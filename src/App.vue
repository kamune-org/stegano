<script setup lang="ts">
import { ref, computed, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
type Mode = "image" | "audio";
type Tab = "encode" | "decode";

const activeMode = ref<Mode>("image");
const activeTab = ref<Tab>("encode");
const isLoading = ref(false);
const errorMessage = ref("");
const successMessage = ref("");

// Image Encode state
const encodeImage = ref<string | null>(null);
const encodeImageName = ref("");
const encodeMessage = ref("");
const encodePassphrase = ref("");
const imageCapacity = ref<number | null>(null);
const encodedImageResult = ref<string | null>(null);

// Image Decode state
const decodeImage = ref<string | null>(null);
const decodeImageName = ref("");
const decodeImagePassphrase = ref("");
const decodedImageMessage = ref("");

// Audio Encode state
const encodeAudio = ref<string | null>(null);
const encodeAudioName = ref("");
const encodeAudioMessage = ref("");
const encodeAudioPassphrase = ref("");
const audioCapacity = ref<number | null>(null);
const encodedAudioResult = ref<string | null>(null);

// Audio Decode state
const decodeAudio = ref<string | null>(null);
const decodeAudioName = ref("");
const decodeAudioPassphrase = ref("");
const decodedAudioMessage = ref("");

// Webcam capture state
const showWebcam = ref(false);
const videoRef = ref<HTMLVideoElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const webcamStream = ref<MediaStream | null>(null);

// Audio recording state
const showRecorder = ref(false);
const isRecording = ref(false);
const mediaRecorder = ref<MediaRecorder | null>(null);
const audioChunks = ref<Blob[]>([]);
const recordingTime = ref(0);
const recordingInterval = ref<number | null>(null);

const messageLength = computed(() => {
    if (activeMode.value === "image") {
        return new TextEncoder().encode(encodeMessage.value).length;
    } else {
        return new TextEncoder().encode(encodeAudioMessage.value).length;
    }
});

const canEncode = computed(() => {
    if (activeMode.value === "image") {
        return (
            encodeImage.value &&
            encodeMessage.value.trim() &&
            encodePassphrase.value &&
            imageCapacity.value !== null &&
            messageLength.value <= imageCapacity.value
        );
    } else {
        return (
            encodeAudio.value &&
            encodeAudioMessage.value.trim() &&
            encodeAudioPassphrase.value &&
            audioCapacity.value !== null &&
            messageLength.value <= audioCapacity.value
        );
    }
});

const canDecode = computed(() => {
    if (activeMode.value === "image") {
        return decodeImage.value && decodeImagePassphrase.value;
    } else {
        return decodeAudio.value && decodeAudioPassphrase.value;
    }
});

function clearMessages() {
    errorMessage.value = "";
    successMessage.value = "";
}

function switchMode(mode: Mode) {
    activeMode.value = mode;
    clearMessages();
    stopWebcam();
    stopRecording();
}

function switchTab(tab: Tab) {
    activeTab.value = tab;
    clearMessages();
    stopWebcam();
    stopRecording();
}

// ============================================================================
// WEBCAM CAPTURE FUNCTIONS
// ============================================================================

async function startWebcam() {
    clearMessages();
    try {
        const stream = await navigator.mediaDevices.getUserMedia({
            video: { facingMode: "environment", width: 1280, height: 720 },
            audio: false,
        });
        webcamStream.value = stream;
        showWebcam.value = true;

        // Wait for next tick to ensure video element is mounted
        setTimeout(() => {
            if (videoRef.value) {
                videoRef.value.srcObject = stream;
                videoRef.value.play();
            }
        }, 100);
    } catch (err) {
        errorMessage.value = `Failed to access webcam: ${err}`;
    }
}

function stopWebcam() {
    if (webcamStream.value) {
        webcamStream.value.getTracks().forEach((track) => track.stop());
        webcamStream.value = null;
    }
    showWebcam.value = false;
}

async function capturePhoto() {
    if (!videoRef.value || !canvasRef.value) return;

    const video = videoRef.value;
    const canvas = canvasRef.value;

    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.drawImage(video, 0, 0);

    // Get base64 without the data URL prefix
    const dataUrl = canvas.toDataURL("image/png");
    const base64 = dataUrl.split(",")[1];

    encodeImage.value = base64;
    encodeImageName.value = `capture_${Date.now()}.png`;

    // Get image capacity
    try {
        const capacity = await invoke<number>("get_image_capacity", {
            imageBase64: base64,
        });
        imageCapacity.value = capacity;
        encodedImageResult.value = null;
        successMessage.value = "Photo captured successfully!";
    } catch (err) {
        errorMessage.value = `Failed to process captured image: ${err}`;
    }

    stopWebcam();
}

// ============================================================================
// AUDIO RECORDING FUNCTIONS
// ============================================================================

async function startRecording() {
    clearMessages();
    try {
        const stream = await navigator.mediaDevices.getUserMedia({
            audio: {
                sampleRate: 44100,
                channelCount: 1,
                echoCancellation: true,
                noiseSuppression: true,
            },
            video: false,
        });

        // Use audio/webm for recording, we'll convert later
        const recorder = new MediaRecorder(stream, {
            mimeType: "audio/webm;codecs=opus",
        });

        audioChunks.value = [];
        recordingTime.value = 0;

        recorder.ondataavailable = (event) => {
            if (event.data.size > 0) {
                audioChunks.value.push(event.data);
            }
        };

        recorder.onstop = async () => {
            stream.getTracks().forEach((track) => track.stop());
            await processRecordedAudio();
        };

        mediaRecorder.value = recorder;
        recorder.start(100); // Collect data every 100ms
        isRecording.value = true;
        showRecorder.value = true;

        // Start timer
        recordingInterval.value = window.setInterval(() => {
            recordingTime.value++;
        }, 1000);
    } catch (err) {
        errorMessage.value = `Failed to access microphone: ${err}`;
    }
}

function stopRecording() {
    if (recordingInterval.value) {
        clearInterval(recordingInterval.value);
        recordingInterval.value = null;
    }

    if (mediaRecorder.value && isRecording.value) {
        mediaRecorder.value.stop();
        isRecording.value = false;
    }

    showRecorder.value = false;
}

function cancelRecording() {
    if (recordingInterval.value) {
        clearInterval(recordingInterval.value);
        recordingInterval.value = null;
    }

    if (mediaRecorder.value && isRecording.value) {
        mediaRecorder.value.stream.getTracks().forEach((track) => track.stop());
        isRecording.value = false;
    }

    audioChunks.value = [];
    showRecorder.value = false;
}

async function processRecordedAudio() {
    try {
        const webmBlob = new Blob(audioChunks.value, { type: "audio/webm" });

        // Convert WebM to WAV using AudioContext
        const arrayBuffer = await webmBlob.arrayBuffer();
        const audioContext = new AudioContext({ sampleRate: 44100 });
        const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

        // Convert to WAV
        const wavBuffer = audioBufferToWav(audioBuffer);
        const base64 = arrayBufferToBase64(new Uint8Array(wavBuffer));

        encodeAudio.value = base64;
        encodeAudioName.value = `recording_${Date.now()}.wav`;

        // Get audio capacity
        const capacity = await invoke<number>("get_audio_capacity", {
            audioBase64: base64,
        });
        audioCapacity.value = capacity;
        encodedAudioResult.value = null;
        successMessage.value = "Audio recorded successfully!";

        await audioContext.close();
    } catch (err) {
        errorMessage.value = `Failed to process recorded audio: ${err}`;
    }
}

// Convert AudioBuffer to WAV format
function audioBufferToWav(buffer: AudioBuffer): ArrayBuffer {
    const numChannels = buffer.numberOfChannels;
    const sampleRate = buffer.sampleRate;
    const format = 1; // PCM
    const bitDepth = 16;

    const bytesPerSample = bitDepth / 8;
    const blockAlign = numChannels * bytesPerSample;

    const dataLength = buffer.length * blockAlign;
    const bufferLength = 44 + dataLength;

    const arrayBuffer = new ArrayBuffer(bufferLength);
    const view = new DataView(arrayBuffer);

    // WAV header
    writeString(view, 0, "RIFF");
    view.setUint32(4, 36 + dataLength, true);
    writeString(view, 8, "WAVE");
    writeString(view, 12, "fmt ");
    view.setUint32(16, 16, true); // fmt chunk size
    view.setUint16(20, format, true);
    view.setUint16(22, numChannels, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * blockAlign, true);
    view.setUint16(32, blockAlign, true);
    view.setUint16(34, bitDepth, true);
    writeString(view, 36, "data");
    view.setUint32(40, dataLength, true);

    // Write audio data
    const offset = 44;
    const channelData: Float32Array[] = [];
    for (let i = 0; i < numChannels; i++) {
        channelData.push(buffer.getChannelData(i));
    }

    for (let i = 0; i < buffer.length; i++) {
        for (let channel = 0; channel < numChannels; channel++) {
            const sample = Math.max(-1, Math.min(1, channelData[channel][i]));
            const intSample = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
            view.setInt16(
                offset + (i * numChannels + channel) * bytesPerSample,
                intSample,
                true,
            );
        }
    }

    return arrayBuffer;
}

function writeString(view: DataView, offset: number, string: string) {
    for (let i = 0; i < string.length; i++) {
        view.setUint8(offset + i, string.charCodeAt(i));
    }
}

function formatTime(seconds: number): string {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

// ============================================================================
// IMAGE FUNCTIONS
// ============================================================================

async function selectEncodeImage() {
    clearMessages();
    try {
        const selected = await open({
            multiple: false,
            filters: [
                {
                    name: "Images",
                    extensions: ["png", "jpg", "jpeg", "bmp", "gif", "webp"],
                },
            ],
        });

        if (selected) {
            const fileData = await readFile(selected);
            const base64 = arrayBufferToBase64(fileData);
            encodeImage.value = base64;
            encodeImageName.value = selected.split("/").pop() || selected;

            // Get image capacity
            const capacity = await invoke<number>("get_image_capacity", {
                imageBase64: base64,
            });
            imageCapacity.value = capacity;
            encodedImageResult.value = null;
        }
    } catch (err) {
        errorMessage.value = `Failed to load image: ${err}`;
    }
}

async function selectDecodeImage() {
    clearMessages();
    try {
        const selected = await open({
            multiple: false,
            filters: [
                {
                    name: "Images",
                    extensions: ["png"],
                },
            ],
        });

        if (selected) {
            const fileData = await readFile(selected);
            const base64 = arrayBufferToBase64(fileData);
            decodeImage.value = base64;
            decodeImageName.value = selected.split("/").pop() || selected;
            decodedImageMessage.value = "";
        }
    } catch (err) {
        errorMessage.value = `Failed to load image: ${err}`;
    }
}

async function encodeMessageIntoImage() {
    if (!canEncode.value) return;

    clearMessages();
    isLoading.value = true;

    try {
        const result = await invoke<string>("encode_message", {
            imageBase64: encodeImage.value,
            message: encodeMessage.value,
            passphrase: encodePassphrase.value,
        });

        encodedImageResult.value = result;
        successMessage.value =
            "Message encoded successfully! Click 'Save Image' to download.";
    } catch (err) {
        errorMessage.value = `Encoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

async function saveEncodedImage() {
    if (!encodedImageResult.value) return;

    try {
        const savePath = await save({
            defaultPath: (() => {
                const d = new Date();
                const pad = (n: number) => String(n).padStart(2, "0");
                const ts = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
                return `image_${ts}.png`;
            })(),
            filters: [
                {
                    name: "PNG Image",
                    extensions: ["png"],
                },
            ],
        });

        if (savePath) {
            const binaryData = base64ToArrayBuffer(encodedImageResult.value);
            await writeFile(savePath, binaryData);
            successMessage.value = `Image saved to ${savePath}`;
        }
    } catch (err) {
        errorMessage.value = `Failed to save image: ${err}`;
    }
}

async function decodeMessageFromImage() {
    if (!canDecode.value) return;

    clearMessages();
    isLoading.value = true;

    try {
        const result = await invoke<string>("decode_message", {
            imageBase64: decodeImage.value,
            passphrase: decodeImagePassphrase.value,
        });

        decodedImageMessage.value = result;
        successMessage.value = "Message decoded successfully!";
    } catch (err) {
        errorMessage.value = `Decoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

// ============================================================================
// AUDIO FUNCTIONS
// ============================================================================

async function selectEncodeAudio() {
    clearMessages();
    try {
        const selected = await open({
            multiple: false,
            filters: [
                {
                    name: "Audio",
                    extensions: ["wav"],
                },
            ],
        });

        if (selected) {
            const fileData = await readFile(selected);
            const base64 = arrayBufferToBase64(fileData);
            encodeAudio.value = base64;
            encodeAudioName.value = selected.split("/").pop() || selected;

            // Get audio capacity
            const capacity = await invoke<number>("get_audio_capacity", {
                audioBase64: base64,
            });
            audioCapacity.value = capacity;
            encodedAudioResult.value = null;
        }
    } catch (err) {
        errorMessage.value = `Failed to load audio: ${err}`;
    }
}

async function selectDecodeAudio() {
    clearMessages();
    try {
        const selected = await open({
            multiple: false,
            filters: [
                {
                    name: "Audio",
                    extensions: ["wav"],
                },
            ],
        });

        if (selected) {
            const fileData = await readFile(selected);
            const base64 = arrayBufferToBase64(fileData);
            decodeAudio.value = base64;
            decodeAudioName.value = selected.split("/").pop() || selected;
            decodedAudioMessage.value = "";
        }
    } catch (err) {
        errorMessage.value = `Failed to load audio: ${err}`;
    }
}

async function encodeMessageIntoAudio() {
    if (!canEncode.value) return;

    clearMessages();
    isLoading.value = true;

    try {
        const result = await invoke<string>("encode_audio_message", {
            audioBase64: encodeAudio.value,
            message: encodeAudioMessage.value,
            passphrase: encodeAudioPassphrase.value,
        });

        encodedAudioResult.value = result;
        successMessage.value =
            "Message encoded successfully! Click 'Save Audio' to download.";
    } catch (err) {
        errorMessage.value = `Encoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

async function saveEncodedAudio() {
    if (!encodedAudioResult.value) return;

    try {
        const savePath = await save({
            defaultPath: (() => {
                const d = new Date();
                const pad = (n: number) => String(n).padStart(2, "0");
                const ts = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
                return `voice_${ts}.wav`;
            })(),
            filters: [
                {
                    name: "WAV Audio",
                    extensions: ["wav"],
                },
            ],
        });

        if (savePath) {
            const binaryData = base64ToArrayBuffer(encodedAudioResult.value);
            await writeFile(savePath, binaryData);
            successMessage.value = `Audio saved to ${savePath}`;
        }
    } catch (err) {
        errorMessage.value = `Failed to save audio: ${err}`;
    }
}

async function decodeMessageFromAudio() {
    if (!canDecode.value) return;

    clearMessages();
    isLoading.value = true;

    try {
        const result = await invoke<string>("decode_audio_message", {
            audioBase64: decodeAudio.value,
            passphrase: decodeAudioPassphrase.value,
        });

        decodedAudioMessage.value = result;
        successMessage.value = "Message decoded successfully!";
    } catch (err) {
        errorMessage.value = `Decoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

function arrayBufferToBase64(buffer: Uint8Array): string {
    let binary = "";
    for (let i = 0; i < buffer.length; i++) {
        binary += String.fromCharCode(buffer[i]);
    }
    return btoa(binary);
}

function base64ToArrayBuffer(base64: string): Uint8Array {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

function copyToClipboard(text: string) {
    if (text) {
        navigator.clipboard
            .writeText(text)
            .then(() => {
                successMessage.value = "Message copied to clipboard!";
                errorMessage.value = "";
            })
            .catch(() => {
                successMessage.value = "";
                errorMessage.value = "Failed to copy to clipboard.";
            });
    }
}

function formatBytes(bytes: number): string {
    if (bytes < 1024) return bytes + " bytes";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

// Cleanup on unmount
onUnmounted(() => {
    stopWebcam();
    cancelRecording();
});
</script>

<template>
    <main class="container">
        <header class="header">
            <h1>🔐 Stegano</h1>
            <p class="subtitle">Hide secret messages in images & audio</p>
        </header>

        <!-- Mode Selector -->
        <div class="mode-selector">
            <button
                :class="['mode-btn', { active: activeMode === 'image' }]"
                @click="switchMode('image')"
            >
                🖼️ Image
            </button>
            <button
                :class="['mode-btn', { active: activeMode === 'audio' }]"
                @click="switchMode('audio')"
            >
                🎵 Audio
            </button>
        </div>

        <!-- Action Tabs -->
        <div class="tabs">
            <button
                :class="['tab', { active: activeTab === 'encode' }]"
                @click="switchTab('encode')"
            >
                📝 Encode
            </button>
            <button
                :class="['tab', { active: activeTab === 'decode' }]"
                @click="switchTab('decode')"
            >
                🔍 Decode
            </button>
        </div>

        <!-- Messages -->
        <div v-if="errorMessage" class="message error">
            {{ errorMessage }}
        </div>
        <div v-if="successMessage" class="message success">
            {{ successMessage }}
        </div>

        <!-- ================================================================ -->
        <!-- WEBCAM MODAL -->
        <!-- ================================================================ -->
        <div v-if="showWebcam" class="modal-overlay" @click.self="stopWebcam">
            <div class="modal">
                <h3>📷 Capture Photo</h3>
                <div class="webcam-container">
                    <video ref="videoRef" autoplay playsinline></video>
                    <canvas ref="canvasRef" style="display: none"></canvas>
                </div>
                <div class="modal-actions">
                    <button class="primary-btn" @click="capturePhoto">
                        📸 Capture
                    </button>
                    <button class="secondary-btn" @click="stopWebcam">
                        Cancel
                    </button>
                </div>
            </div>
        </div>

        <!-- ================================================================ -->
        <!-- AUDIO RECORDER MODAL -->
        <!-- ================================================================ -->
        <div
            v-if="showRecorder"
            class="modal-overlay"
            @click.self="cancelRecording"
        >
            <div class="modal">
                <h3>🎙️ Record Audio</h3>
                <div class="recorder-container">
                    <div
                        class="recording-indicator"
                        :class="{ active: isRecording }"
                    >
                        <span class="pulse"></span>
                        <span class="time">{{
                            formatTime(recordingTime)
                        }}</span>
                    </div>
                    <p class="recording-hint">
                        {{
                            isRecording
                                ? "Recording in progress..."
                                : "Processing..."
                        }}
                    </p>
                </div>
                <div class="modal-actions">
                    <button
                        v-if="isRecording"
                        class="primary-btn stop-btn"
                        @click="stopRecording"
                    >
                        ⏹️ Stop Recording
                    </button>
                    <button class="secondary-btn" @click="cancelRecording">
                        Cancel
                    </button>
                </div>
            </div>
        </div>

        <!-- ================================================================ -->
        <!-- IMAGE MODE -->
        <!-- ================================================================ -->

        <!-- Image Encode Panel -->
        <div
            v-if="activeMode === 'image' && activeTab === 'encode'"
            class="panel"
        >
            <div class="file-select">
                <button class="select-btn" @click="selectEncodeImage">
                    📁 {{ encodeImage ? "Change Image" : "Select Image" }}
                </button>
                <button class="select-btn capture-btn" @click="startWebcam">
                    📷 Capture
                </button>
                <span v-if="encodeImageName" class="file-name">{{
                    encodeImageName
                }}</span>
            </div>

            <div v-if="encodeImage" class="preview image-preview">
                <img
                    :src="'data:image/png;base64,' + encodeImage"
                    alt="Selected image"
                />
            </div>

            <div v-if="imageCapacity !== null" class="capacity-info">
                <span
                    :class="{
                        warning:
                            messageLength > imageCapacity * 0.8 &&
                            messageLength <= imageCapacity,
                        error: messageLength > imageCapacity,
                    }"
                >
                    {{ formatBytes(messageLength) }} /
                    {{ formatBytes(imageCapacity) }}
                </span>
            </div>

            <div class="input-group">
                <label for="encode-message">Secret Message</label>
                <textarea
                    id="encode-message"
                    v-model="encodeMessage"
                    placeholder="Enter your secret message..."
                    rows="4"
                ></textarea>
            </div>

            <div class="input-group">
                <label for="encode-passphrase">Passphrase</label>
                <input
                    id="encode-passphrase"
                    v-model="encodePassphrase"
                    type="password"
                    placeholder="Enter a strong passphrase..."
                />
            </div>

            <div class="actions">
                <button
                    class="primary-btn"
                    :disabled="!canEncode || isLoading"
                    @click="encodeMessageIntoImage"
                >
                    {{ isLoading ? "Encoding..." : "Encode Message" }}
                </button>
                <button
                    v-if="encodedImageResult"
                    class="secondary-btn"
                    @click="saveEncodedImage"
                >
                    💾 Save Image
                </button>
            </div>

            <div v-if="encodedImageResult" class="result-preview">
                <h3>Result Preview</h3>
                <img
                    :src="'data:image/png;base64,' + encodedImageResult"
                    alt="Encoded image"
                />
            </div>
        </div>

        <!-- Image Decode Panel -->
        <div
            v-if="activeMode === 'image' && activeTab === 'decode'"
            class="panel"
        >
            <div class="file-select">
                <button class="select-btn" @click="selectDecodeImage">
                    📁 {{ decodeImage ? "Change Image" : "Select Image" }}
                </button>
                <span v-if="decodeImageName" class="file-name">{{
                    decodeImageName
                }}</span>
            </div>

            <div v-if="decodeImage" class="preview image-preview">
                <img
                    :src="'data:image/png;base64,' + decodeImage"
                    alt="Selected image"
                />
            </div>

            <div class="input-group">
                <label for="decode-image-passphrase">Passphrase</label>
                <input
                    id="decode-image-passphrase"
                    v-model="decodeImagePassphrase"
                    type="password"
                    placeholder="Enter the passphrase..."
                />
            </div>

            <div class="actions">
                <button
                    class="primary-btn"
                    :disabled="!canDecode || isLoading"
                    @click="decodeMessageFromImage"
                >
                    {{ isLoading ? "Decoding..." : "Decode Message" }}
                </button>
            </div>

            <div v-if="decodedImageMessage" class="decoded-result">
                <h3>Decoded Message</h3>
                <div class="message-box">
                    <pre>{{ decodedImageMessage }}</pre>
                    <button
                        class="copy-btn"
                        @click="copyToClipboard(decodedImageMessage)"
                        title="Copy to clipboard"
                    >
                        📋
                    </button>
                </div>
            </div>
        </div>

        <!-- ================================================================ -->
        <!-- AUDIO MODE -->
        <!-- ================================================================ -->

        <!-- Audio Encode Panel -->
        <div
            v-if="activeMode === 'audio' && activeTab === 'encode'"
            class="panel"
        >
            <div class="info-box">
                <p>📌 Only WAV files are supported for audio steganography.</p>
            </div>

            <div class="file-select">
                <button class="select-btn" @click="selectEncodeAudio">
                    📁 {{ encodeAudio ? "Change Audio" : "Select WAV" }}
                </button>
                <button class="select-btn capture-btn" @click="startRecording">
                    🎙️ Record
                </button>
                <span v-if="encodeAudioName" class="file-name">{{
                    encodeAudioName
                }}</span>
            </div>

            <div v-if="encodeAudio" class="preview audio-preview">
                <div class="audio-icon">🎵</div>
                <span class="audio-name">{{ encodeAudioName }}</span>
                <audio controls :src="'data:audio/wav;base64,' + encodeAudio">
                    Your browser does not support the audio element.
                </audio>
            </div>

            <div v-if="audioCapacity !== null" class="capacity-info">
                <span
                    :class="{
                        warning:
                            messageLength > audioCapacity * 0.8 &&
                            messageLength <= audioCapacity,
                        error: messageLength > audioCapacity,
                    }"
                >
                    {{ formatBytes(messageLength) }} /
                    {{ formatBytes(audioCapacity) }}
                </span>
            </div>

            <div class="input-group">
                <label for="encode-audio-message">Secret Message</label>
                <textarea
                    id="encode-audio-message"
                    v-model="encodeAudioMessage"
                    placeholder="Enter your secret message..."
                    rows="4"
                ></textarea>
            </div>

            <div class="input-group">
                <label for="encode-audio-passphrase">Passphrase</label>
                <input
                    id="encode-audio-passphrase"
                    v-model="encodeAudioPassphrase"
                    type="password"
                    placeholder="Enter a strong passphrase..."
                />
            </div>

            <div class="actions">
                <button
                    class="primary-btn"
                    :disabled="!canEncode || isLoading"
                    @click="encodeMessageIntoAudio"
                >
                    {{ isLoading ? "Encoding..." : "Encode Message" }}
                </button>
                <button
                    v-if="encodedAudioResult"
                    class="secondary-btn"
                    @click="saveEncodedAudio"
                >
                    💾 Save Audio
                </button>
            </div>

            <div v-if="encodedAudioResult" class="result-preview">
                <h3>Result Preview</h3>
                <div class="audio-preview">
                    <div class="audio-icon">🎵</div>
                    <span class="audio-name">Encoded Audio</span>
                    <audio
                        controls
                        :src="'data:audio/wav;base64,' + encodedAudioResult"
                    >
                        Your browser does not support the audio element.
                    </audio>
                </div>
            </div>
        </div>

        <!-- Audio Decode Panel -->
        <div
            v-if="activeMode === 'audio' && activeTab === 'decode'"
            class="panel"
        >
            <div class="info-box">
                <p>📌 Select a WAV file that contains a hidden message.</p>
            </div>

            <div class="file-select">
                <button class="select-btn" @click="selectDecodeAudio">
                    📁 {{ decodeAudio ? "Change Audio" : "Select WAV" }}
                </button>
                <span v-if="decodeAudioName" class="file-name">{{
                    decodeAudioName
                }}</span>
            </div>

            <div v-if="decodeAudio" class="preview audio-preview">
                <div class="audio-icon">🎵</div>
                <span class="audio-name">{{ decodeAudioName }}</span>
                <audio controls :src="'data:audio/wav;base64,' + decodeAudio">
                    Your browser does not support the audio element.
                </audio>
            </div>

            <div class="input-group">
                <label for="decode-audio-passphrase">Passphrase</label>
                <input
                    id="decode-audio-passphrase"
                    v-model="decodeAudioPassphrase"
                    type="password"
                    placeholder="Enter the passphrase..."
                />
            </div>

            <div class="actions">
                <button
                    class="primary-btn"
                    :disabled="!canDecode || isLoading"
                    @click="decodeMessageFromAudio"
                >
                    {{ isLoading ? "Decoding..." : "Decode Message" }}
                </button>
            </div>

            <div v-if="decodedAudioMessage" class="decoded-result">
                <h3>Decoded Message</h3>
                <div class="message-box">
                    <pre>{{ decodedAudioMessage }}</pre>
                    <button
                        class="copy-btn"
                        @click="copyToClipboard(decodedAudioMessage)"
                        title="Copy to clipboard"
                    >
                        📋
                    </button>
                </div>
            </div>
        </div>
    </main>
</template>

<style src="./styles/app.css"></style>
