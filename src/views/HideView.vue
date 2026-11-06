<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { formatBytes } from "../lib/format";
import { canCaptureMedia, isTauri } from "../api/pickers";
import { useLayout } from "../lib/layout";
import { useHideStore } from "../stores/hide";
import AudioRecorder from "../components/AudioRecorder.vue";
import CameraCapture from "../components/CameraCapture.vue";
import CapacityMeter from "../components/CapacityMeter.vue";
import CarrierCard from "../components/CarrierCard.vue";
import PassphraseField from "../components/PassphraseField.vue";
import PassphraseGenerator from "../components/PassphraseGenerator.vue";
import ProgressSheet from "../components/ProgressSheet.vue";
import ResultSheet from "../components/ResultSheet.vue";
import ScaleSelector from "../components/ScaleSelector.vue";
import SecretCard from "../components/SecretCard.vue";
import StepHeader from "../components/StepHeader.vue";

defineProps<{ carrierHot?: boolean; secretHot?: boolean }>();

const hide = useHideStore();
const { desktop } = useLayout();
const capture = canCaptureMedia();
const browserDrop = !isTauri();
const cameraOpen = ref(false);
const recorderOpen = ref(false);
const carrierCard = ref<{ el: HTMLElement | null } | null>(null);
const secretCard = ref<{ el: HTMLElement | null } | null>(null);

const holds = computed(() => {
    const view = hide.capacityView;
    const info = hide.effectiveInfo;
    if (!view || !info) return "";
    const mark = view.approx ? "≈ " : "";
    return `Holds up to ${mark}${formatBytes(view.capacity)} · Safe up to ${mark}${formatBytes(view.recommended)}`;
});

const holdsShort = computed(() => {
    const view = hide.capacityView;
    if (!view || !hide.effectiveInfo) return "";
    const mark = view.approx ? "≈ " : "";
    return `holds ${mark}${formatBytes(view.capacity)}, safe ${mark}${formatBytes(view.recommended)}`;
});

const mismatch = computed(
    () => hide.confirmPass.length > 0 && hide.confirmPass !== hide.passphrase,
);

const steps = computed(() => [
    { label: "Cover", done: hide.carrier.status === "ready" },
    { label: "Secret", done: hide.hasPayload },
    { label: "Passphrase", done: hide.passphrase.length > 0 && hide.passphrase === hide.confirmPass },
]);

watch(cameraOpen, (isOpen, was) => {
    if (isOpen && !was) hide.lock();
    if (!isOpen && was) hide.unlock();
});
watch(recorderOpen, (isOpen, was) => {
    if (isOpen && !was) hide.lock();
    if (!isOpen && was) hide.unlock();
});

onBeforeUnmount(() => {
    if (cameraOpen.value) hide.unlock();
    if (recorderOpen.value) hide.unlock();
});

function onCaptured(name: string, bytes: Uint8Array, label: string): void {
    void hide.useCaptured(name, bytes, label);
}

defineExpose({
    carrierElement: () => carrierCard.value?.el ?? null,
    secretElement: () => secretCard.value?.el ?? null,
});
</script>

<template>
    <div id="panel-hide" role="tabpanel" aria-label="Hide" class="column view">
        <StepHeader v-if="!desktop" :steps="steps" />
        <div class="pane pane-cover">
            <CarrierCard
                ref="carrierCard"
                :status="hide.carrier.status"
                :name="hide.carrier.name"
                :info="hide.carrier.info"
                :error="hide.carrier.error"
                :preview-url="hide.carrier.previewUrl"
                :peaks="hide.carrier.peaks"
                :preview-loading="hide.carrier.previewLoading"
                :hot="carrierHot"
                :disabled="hide.frozen"
                :allow-capture="capture"
                :browser-drop="browserDrop"
                :holds="holds"
                :holds-short="holdsShort"
                @choose="hide.chooseCarrier"
                @remove="hide.removeCarrier"
                @retry="hide.retryCarrier"
                @photo="cameraOpen = true"
                @record="recorderOpen = true"
                @files="hide.addBrowserFiles($event, 'carrier')"
            >
                <ScaleSelector
                    v-if="hide.carrier.info?.resizable"
                    :model-value="hide.scale"
                    :info="hide.carrier.info"
                    :prepared="hide.prepared?.scale === hide.scale ? hide.prepared.info : null"
                    :preparing="hide.preparing"
                    :cancelling="hide.prepareCancelling"
                    :fraction="hide.prepareFraction"
                    :stage="hide.prepareStage"
                    :error="hide.prepareError"
                    :disabled="hide.frozen"
                    @update:model-value="hide.selectScale"
                    @cancel="hide.cancelScale"
                />
            </CarrierCard>
        </div>

        <div class="pane pane-form">
            <div class="pane-scroll">
                <SecretCard
                    ref="secretCard"
                    v-model="hide.text"
                    :files="hide.files"
                    :bytes="hide.textBytes"
                    :hot="secretHot"
                    :disabled="hide.frozen"
                    :browser-drop="browserDrop"
                    @add="hide.addFiles"
                    @remove="hide.removeFile"
                    @files="hide.addBrowserFiles($event, 'secret')"
                >
                    <template v-if="!desktop && hide.capacityView">
                        <hr class="divider" />
                        <CapacityMeter
                            :payload="hide.hasPayload ? hide.payload : null"
                            :capacity="hide.capacityView.capacity"
                            :recommended="hide.capacityView.recommended"
                            :estimating="hide.estimating"
                            :approx="hide.capacityView.approx"
                            :zone="hide.zone"
                        />
                    </template>
                </SecretCard>

                <section class="card flex flex-col gap-4" aria-labelledby="pass-heading">
                    <h2 id="pass-heading" class="t-section">Passphrase</h2>
                    <PassphraseField
                        id="hide-pass"
                        v-model="hide.passphrase"
                        v-model:visible="hide.showPass"
                        label="Passphrase"
                        show-strength
                        :disabled="hide.frozen"
                    />
                    <PassphraseField
                        id="hide-confirm"
                        v-model="hide.confirmPass"
                        v-model:visible="hide.showConfirm"
                        label="Confirm"
                        :disabled="hide.frozen"
                        :error="mismatch ? `Passphrases don't match` : ''"
                    />
                    <PassphraseGenerator :passphrase="hide.passphrase" :disabled="hide.frozen" @generate="hide.applyGenerated" />
                    <hr class="divider" />
                    <h2 v-if="desktop" class="t-section">Options</h2>
                    <label class="switch-row" for="compress-payload">
                        <span class="min-w-0">
                            <span class="block t-body font-medium">Compress payload</span>
                            <span class="t-caption block">Shrinks it before hiding. The size shown is before compression.</span>
                        </span>
                        <input
                            id="compress-payload"
                            v-model="hide.compress"
                            class="switch"
                            type="checkbox"
                            role="switch"
                            :disabled="hide.frozen"
                        />
                    </label>
                </section>

                <p class="t-caption foot-note px-1 text-center">
                    Share the result as a file. Apps that recompress images or audio destroy the hidden data.
                </p>
            </div>
            <div v-if="desktop" class="pane-footer">
                <CapacityMeter
                    v-if="hide.capacityView"
                    :payload="hide.hasPayload ? hide.payload : null"
                    :capacity="hide.capacityView.capacity"
                    :recommended="hide.capacityView.recommended"
                    :estimating="hide.estimating"
                    :approx="hide.capacityView.approx"
                    :zone="hide.zone"
                />
                <p v-else class="t-caption">Capacity appears once you choose a cover file.</p>
            </div>
        </div>

        <CameraCapture v-model:open="cameraOpen" @use="(name, bytes) => onCaptured(name, bytes, 'Importing photo')" />
        <AudioRecorder v-model:open="recorderOpen" @use="(name, bytes) => onCaptured(name, bytes, 'Importing recording')" />
        <ProgressSheet
            :open="hide.progressOpen"
            :title="hide.progressTitle"
            :label="hide.progressLabel"
            :fraction="hide.progressFraction"
            :started-at="hide.progressStartedAt"
            :cancelling="hide.progressCancelling"
            @cancel="hide.cancelProgress"
        />
        <ResultSheet
            :open="hide.phase === 'result'"
            :report="hide.report"
            :saving="hide.saving"
            :saved="!!hide.savedPath"
            :can-show-folder="hide.desktop"
            @save="hide.saveResult"
            @show="hide.showSaved"
            @done="hide.finishResult"
        />
    </div>
</template>
