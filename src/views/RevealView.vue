<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { isTauri } from "../api/pickers";
import { useRevealStore } from "../stores/reveal";
import CarrierCard from "../components/CarrierCard.vue";
import InlineAction from "../components/InlineAction.vue";
import PassphraseField from "../components/PassphraseField.vue";
import ProgressSheet from "../components/ProgressSheet.vue";
import RevealResults from "../components/RevealResults.vue";

defineProps<{ carrierHot?: boolean }>();

const reveal = useRevealStore();
const browserDrop = !isTauri();
const carrierCard = ref<{ el: HTMLElement | null } | null>(null);

defineExpose({
    carrierElement: () => carrierCard.value?.el ?? null,
});

watch(
    () => reveal.revealed,
    (shown) => {
        if (!shown) return;
        void nextTick(() => {
            const heading = document.getElementById("results-heading");
            if (!(heading instanceof HTMLElement)) return;
            const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
            heading.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
            heading.focus({ preventScroll: true });
        });
    },
);
</script>

<template>
    <div id="panel-reveal" role="tabpanel" aria-label="Reveal" class="column pt-2 md:pt-0">
        <div class="screen-head">
            <h1 class="t-screen">Reveal a secret</h1>
            <p class="mt-1 text-[15px] text-muted">Pick a file that hides something and enter its passphrase.</p>
        </div>
        <CarrierCard
            ref="carrierCard"
            :status="reveal.carrier.status"
            :name="reveal.carrier.name"
            :info="reveal.carrier.info"
            :error="reveal.carrier.error"
            :preview-url="reveal.carrier.previewUrl"
            :peaks="reveal.carrier.peaks"
            :preview-loading="reveal.carrier.previewLoading"
            :hot="carrierHot"
            :disabled="reveal.frozen"
            :browser-drop="browserDrop"
            @choose="reveal.chooseCarrier"
            @remove="reveal.removeCarrier"
            @retry="reveal.retryCarrier"
            @files="reveal.addBrowserFiles"
        />
        <section class="card flex flex-col gap-4" aria-labelledby="reveal-pass-heading">
            <h2 id="reveal-pass-heading" class="t-section">Passphrase</h2>
            <PassphraseField
                id="reveal-pass"
                v-model="reveal.passphrase"
                v-model:visible="reveal.showPass"
                label="Passphrase"
                hide-label
                :disabled="reveal.frozen"
                :error="reveal.fieldError"
                @enter="reveal.run"
            />
        </section>
        <InlineAction mode="reveal" />
        <RevealResults
            v-if="reveal.revealed"
            :entries="reveal.entries"
            :copied="reveal.copiedIndex"
            :saved="reveal.savedIndexes"
            :disabled="reveal.frozen"
            @copy="(entry, area) => reveal.copyEntry(entry, area)"
            @save="reveal.saveFile"
            @clear="reveal.clearAll"
        />
        <p class="t-caption px-1 text-center">Leaving this tab wipes the revealed message and files.</p>
        <ProgressSheet
            :open="reveal.progressOpen"
            title="Revealing"
            :label="reveal.progressLabel"
            :fraction="reveal.jobFraction"
            :started-at="reveal.jobStartedAt"
            :cancelling="reveal.jobCancelling"
            @cancel="reveal.cancelProgress"
        />
    </div>
</template>
