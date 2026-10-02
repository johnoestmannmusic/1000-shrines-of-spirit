// Runs the track's WASM on the audio thread. Each 128-frame quantum asks the
// module for the next samples; the module itself never knows about blocks,
// so the browser hears exactly what the native renderer writes to disk.

class Track0010 extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { wasm, seeds, settings } = options.processorOptions;
    const { exports } = new WebAssembly.Instance(new WebAssembly.Module(wasm), {});
    this.wasm = exports;
    const args = seeds.flatMap((s) => {
      const v = BigInt(s);
      return [Number(v >> 32n), Number(v & 0xffffffffn)];
    });
    this.player = exports.sos_new(...args, settings.scale, settings.chords, settings.pace, settings.kit);
    this.port.postMessage("ready");
  }

  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    const frames = left.length;
    const ptr = this.wasm.sos_render(this.player, frames);
    // Fresh view every call: memory may have grown and moved.
    const buf = new Float32Array(this.wasm.memory.buffer, ptr, frames * 2);
    for (let i = 0; i < frames; i++) {
      left[i] = buf[2 * i];
      right[i] = buf[2 * i + 1];
    }
    return true;
  }
}

registerProcessor("track-0010", Track0010);
