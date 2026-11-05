class RecorderProcessor extends AudioWorkletProcessor {
    process(inputs) {
        const channels = inputs[0];
        const channel = channels && channels[0];
        if (channel && channel.length > 0) {
            const copy = new Float32Array(channel.length);
            copy.set(channel);
            this.port.postMessage(copy);
        }
        return true;
    }
}

registerProcessor("recorder-processor", RecorderProcessor);
