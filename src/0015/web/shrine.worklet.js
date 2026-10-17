// Runs the shrine's music on the audio thread.
//
// Plain WebAssembly only (HC006): the module is instantiated here and asked for
// interleaved stereo samples. It never knows about blocks, so native and browser
// output are identical.
//
// The page's Learn mode can ask for two more things, neither of which changes
// the music: the engine's moving parts (posted now and then while it is
// watching), and to hear one layer on its own.

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
    this.live = false;
    this.port.onmessage = (e) => {
      const m = e.data || {};
      if (typeof m.live === "boolean") this.live = m.live;
      if (typeof m.solo === "number" && typeof this.shrine.shrine_set_solo === "function") {
        this.shrine.shrine_set_solo(this.player, m.solo);
      }
    };
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
    if (this.samples % 4096 < frames) {
      this.port.postMessage(this.samples);
      if (this.live && typeof this.shrine.shrine_learn_state === "function") {
        const n = this.shrine.shrine_learn_state_len();
        const s = new Float64Array(this.shrine.memory.buffer, this.shrine.shrine_learn_state(this.player), n);
        this.port.postMessage({ state: Float64Array.from(s) });
      }
    }
    return true;
  }
}

registerProcessor("shrinegen", ShrineGen);
