// Runs the shrine's music on the audio thread.
//
// Plain WebAssembly only (HC006): the module is instantiated here and asked for
// interleaved stereo samples. It never knows about blocks, so native and browser
// output are identical.

class ShrineGen extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { wasm } = options.processorOptions;
    const module = new WebAssembly.Module(wasm);
    const instance = new WebAssembly.Instance(module, {});
    this.shrine = instance.exports;
    // Mode 1: audio only. Default recipe until the artifact bakes its own.
    this.player = this.shrine.shrine_new(0, 0, 1);
    this.samples = 0;
    this.port.postMessage("ready");
  }

  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    const frames = left.length;
    const ptr = this.shrine.shrine_render_audio(this.player, frames);
    // Fresh view each call: rendering may have grown memory.
    const buf = new Float32Array(this.shrine.memory.buffer, ptr, frames * 2);
    for (let i = 0; i < frames; i++) {
      left[i] = buf[2 * i];
      right[i] = buf[2 * i + 1];
    }
    this.samples += frames;
    // Report the clock now and then, so the page can keep the visuals in step.
    if (this.samples % 4096 < frames) this.port.postMessage(this.samples);
    return true;
  }
}

registerProcessor("shrinegen", ShrineGen);
