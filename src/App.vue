<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";

type Tab = "encode" | "decode";

const activeTab = ref<Tab>("encode");
const isLoading = ref(false);
const errorMessage = ref("");
const successMessage = ref("");

// Encode state
const encodeImage = ref<string | null>(null);
const encodeImageName = ref("");
const encodeMessage = ref("");
const encodePassphrase = ref("");
const imageCapacity = ref<number | null>(null);
const encodedResult = ref<string | null>(null);

// Decode state
const decodeImage = ref<string | null>(null);
const decodeImageName = ref("");
const decodePassphrase = ref("");
const decodedMessage = ref("");

const messageLength = computed(() => {
    return new TextEncoder().encode(encodeMessage.value).length;
});

const canEncode = computed(() => {
    return (
        encodeImage.value &&
        encodeMessage.value.trim() &&
        encodePassphrase.value &&
        imageCapacity.value !== null &&
        messageLength.value <= imageCapacity.value
    );
});

const canDecode = computed(() => {
    return decodeImage.value && decodePassphrase.value;
});

function clearMessages() {
    errorMessage.value = "";
    successMessage.value = "";
}

function switchTab(tab: Tab) {
    activeTab.value = tab;
    clearMessages();
}

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
            encodedResult.value = null;
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
            decodedMessage.value = "";
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

        encodedResult.value = result;
        successMessage.value =
            "Message encoded successfully! Click 'Save Image' to download.";
    } catch (err) {
        errorMessage.value = `Encoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

async function saveEncodedImage() {
    if (!encodedResult.value) return;

    try {
        const savePath = await save({
            defaultPath: "stegano_output.png",
            filters: [
                {
                    name: "PNG Image",
                    extensions: ["png"],
                },
            ],
        });

        if (savePath) {
            const binaryData = base64ToArrayBuffer(encodedResult.value);
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
            passphrase: decodePassphrase.value,
        });

        decodedMessage.value = result;
        successMessage.value = "Message decoded successfully!";
    } catch (err) {
        errorMessage.value = `Decoding failed: ${err}`;
    } finally {
        isLoading.value = false;
    }
}

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

function copyToClipboard() {
    if (decodedMessage.value) {
        navigator.clipboard.writeText(decodedMessage.value);
        successMessage.value = "Message copied to clipboard!";
    }
}
</script>

<template>
    <main class="container">
        <header class="header">
            <h1>🔐 Stegano</h1>
            <p class="subtitle">Hide secret messages in images</p>
        </header>

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

        <div v-if="errorMessage" class="message error">
            {{ errorMessage }}
        </div>
        <div v-if="successMessage" class="message success">
            {{ successMessage }}
        </div>

        <!-- Encode Panel -->
        <div v-if="activeTab === 'encode'" class="panel">
            <div class="image-select">
                <button class="select-btn" @click="selectEncodeImage">
                    {{ encodeImage ? "Change Image" : "Select Image" }}
                </button>
                <span v-if="encodeImageName" class="file-name">{{
                    encodeImageName
                }}</span>
            </div>

            <div v-if="encodeImage" class="image-preview">
                <img
                    :src="'data:image/png;base64,' + encodeImage"
                    alt="Selected image"
                />
            </div>

            <div v-if="imageCapacity !== null" class="capacity-info">
                <span
                    :class="{
                        warning: messageLength > imageCapacity * 0.8,
                        error: messageLength > imageCapacity,
                    }"
                >
                    {{ messageLength }} / {{ imageCapacity }} bytes
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
                    v-if="encodedResult"
                    class="secondary-btn"
                    @click="saveEncodedImage"
                >
                    💾 Save Image
                </button>
            </div>

            <div v-if="encodedResult" class="result-preview">
                <h3>Result Preview</h3>
                <img
                    :src="'data:image/png;base64,' + encodedResult"
                    alt="Encoded image"
                />
            </div>
        </div>

        <!-- Decode Panel -->
        <div v-if="activeTab === 'decode'" class="panel">
            <div class="image-select">
                <button class="select-btn" @click="selectDecodeImage">
                    {{ decodeImage ? "Change Image" : "Select Image" }}
                </button>
                <span v-if="decodeImageName" class="file-name">{{
                    decodeImageName
                }}</span>
            </div>

            <div v-if="decodeImage" class="image-preview">
                <img
                    :src="'data:image/png;base64,' + decodeImage"
                    alt="Selected image"
                />
            </div>

            <div class="input-group">
                <label for="decode-passphrase">Passphrase</label>
                <input
                    id="decode-passphrase"
                    v-model="decodePassphrase"
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

            <div v-if="decodedMessage" class="decoded-result">
                <h3>Decoded Message</h3>
                <div class="message-box">
                    <pre>{{ decodedMessage }}</pre>
                    <button
                        class="copy-btn"
                        @click="copyToClipboard"
                        title="Copy to clipboard"
                    >
                        📋
                    </button>
                </div>
            </div>
        </div>
    </main>
</template>

<style>
:root {
    font-family: "Inter", "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
    font-size: 16px;
    line-height: 1.6;
    font-weight: 400;

    color: #e4e4e7;
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%);
    min-height: 100vh;

    font-synthesis: none;
    text-rendering: optimizeLegibility;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
}

* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

body {
    margin: 0;
    min-height: 100vh;
}

.container {
    max-width: 600px;
    margin: 0 auto;
    padding: 2rem;
}

.header {
    text-align: center;
    margin-bottom: 2rem;
}

.header h1 {
    font-size: 2.5rem;
    font-weight: 700;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
    margin-bottom: 0.5rem;
}

.subtitle {
    color: #a1a1aa;
    font-size: 1rem;
}

.tabs {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
    background: rgba(255, 255, 255, 0.05);
    padding: 0.5rem;
    border-radius: 12px;
}

.tab {
    flex: 1;
    padding: 0.75rem 1.5rem;
    border: none;
    background: transparent;
    color: #a1a1aa;
    font-size: 1rem;
    font-weight: 500;
    cursor: pointer;
    border-radius: 8px;
    transition: all 0.2s ease;
}

.tab:hover {
    color: #e4e4e7;
    background: rgba(255, 255, 255, 0.05);
}

.tab.active {
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
}

.panel {
    background: rgba(255, 255, 255, 0.05);
    border-radius: 16px;
    padding: 1.5rem;
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
}

.message {
    padding: 1rem;
    border-radius: 8px;
    margin-bottom: 1rem;
    font-size: 0.9rem;
}

.message.error {
    background: rgba(239, 68, 68, 0.2);
    border: 1px solid rgba(239, 68, 68, 0.5);
    color: #fca5a5;
}

.message.success {
    background: rgba(34, 197, 94, 0.2);
    border: 1px solid rgba(34, 197, 94, 0.5);
    color: #86efac;
}

.image-select {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1rem;
}

.select-btn {
    padding: 0.75rem 1.5rem;
    background: rgba(255, 255, 255, 0.1);
    border: 1px dashed rgba(255, 255, 255, 0.3);
    color: #e4e4e7;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.9rem;
    transition: all 0.2s ease;
}

.select-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(255, 255, 255, 0.5);
}

.file-name {
    color: #a1a1aa;
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.image-preview {
    margin-bottom: 1rem;
    border-radius: 8px;
    overflow: hidden;
    background: rgba(0, 0, 0, 0.2);
    display: flex;
    justify-content: center;
    align-items: center;
    max-height: 200px;
}

.image-preview img {
    max-width: 100%;
    max-height: 200px;
    object-fit: contain;
}

.capacity-info {
    text-align: right;
    font-size: 0.85rem;
    color: #86efac;
    margin-bottom: 1rem;
}

.capacity-info .warning {
    color: #fbbf24;
}

.capacity-info .error {
    color: #f87171;
}

.input-group {
    margin-bottom: 1rem;
}

.input-group label {
    display: block;
    margin-bottom: 0.5rem;
    font-size: 0.9rem;
    color: #a1a1aa;
    font-weight: 500;
}

.input-group input,
.input-group textarea {
    width: 100%;
    padding: 0.75rem 1rem;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: #e4e4e7;
    font-size: 1rem;
    font-family: inherit;
    transition: all 0.2s ease;
}

.input-group input:focus,
.input-group textarea:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.2);
}

.input-group textarea {
    resize: vertical;
    min-height: 100px;
}

.actions {
    display: flex;
    gap: 1rem;
    margin-top: 1.5rem;
}

.primary-btn {
    flex: 1;
    padding: 0.875rem 1.5rem;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    border: none;
    border-radius: 8px;
    color: white;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
}

.primary-btn:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4);
}

.primary-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}

.secondary-btn {
    padding: 0.875rem 1.5rem;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    color: #e4e4e7;
    font-size: 1rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
}

.secondary-btn:hover {
    background: rgba(255, 255, 255, 0.15);
}

.result-preview {
    margin-top: 1.5rem;
    padding-top: 1.5rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.result-preview h3 {
    font-size: 1rem;
    color: #a1a1aa;
    margin-bottom: 1rem;
}

.result-preview img {
    max-width: 100%;
    border-radius: 8px;
}

.decoded-result {
    margin-top: 1.5rem;
    padding-top: 1.5rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.decoded-result h3 {
    font-size: 1rem;
    color: #a1a1aa;
    margin-bottom: 1rem;
}

.message-box {
    position: relative;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 8px;
    padding: 1rem;
    padding-right: 3rem;
}

.message-box pre {
    white-space: pre-wrap;
    word-wrap: break-word;
    font-family: "Fira Code", "Monaco", monospace;
    font-size: 0.9rem;
    color: #86efac;
    margin: 0;
}

.copy-btn {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    padding: 0.5rem;
    background: transparent;
    border: none;
    cursor: pointer;
    font-size: 1.2rem;
    opacity: 0.7;
    transition: opacity 0.2s ease;
}

.copy-btn:hover {
    opacity: 1;
}
</style>
