// Plays the shrine's music on the audio thread.
//
// The music itself is computed ahead of time in background workers (one per
// shrine, on the page): each renders its shrine's samples in 4096-frame
// chunks and sends them here over its own message port. This thread only
// plays them, so it never stalls while a shrine's music is being built, and
// moving to another shrine is a crossfade from one stream to the next,
// without a gap.
//
// Plain WebAssembly only (HC006): the workers run the engine; native and
// browser output are identical sample for sample.

const ACK_EVERY = 2048;   // frames between "played" reports to a stream's worker

class ShrinePlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    // Streams by id: queued chunks, the read position, frames played.
    this.streams = new Map();
    this.current = null;     // the stream being heard
    this.fading = null;      // { from, pos, len }: crossfading from `from` to current
    this.out = 0;            // frames output in total (the page's clock)
    this.port.onmessage = (e) => {
      const m = e.data || {};
      if (m.connect !== undefined && m.port) {
        const s = { queue: [], at: 0, played: 0, acked: 0, port: m.port };
        m.port.onmessage = (ev) => {
          if (ev.data && ev.data.frames) s.queue.push(ev.data.frames);
        };
        this.streams.set(m.connect, s);
      }
      if (m.play !== undefined) {
        this.current = m.play;
      }
      if (m.crossfade) {
        const { to, seconds } = m.crossfade;
        if (this.current !== null && this.current !== to) {
          this.fading = { from: this.current, pos: 0, len: Math.max(1, Math.round(seconds * sampleRate)) };
        }
        this.current = to;
      }
      if (m.drop !== undefined) {
        const s = this.streams.get(m.drop);
        if (s) s.port.close();
        this.streams.delete(m.drop);
        if (this.fading && this.fading.from === m.drop) this.fading = null;
      }
    };
    this.port.postMessage("ready");
  }

  // The next stereo frame of stream `id` (silence if it has run dry).
  frame(id) {
    const s = this.streams.get(id);
    if (!s) return [0, 0];
    while (s.queue.length && s.at >= s.queue[0].length) {
      s.queue.shift();
      s.at = 0;
    }
    if (!s.queue.length) return [0, 0];
    const c = s.queue[0];
    const l = c[s.at], r = c[s.at + 1];
    s.at += 2;
    s.played += 1;
    if (s.played - s.acked >= ACK_EVERY) {
      s.acked = s.played;
      s.port.postMessage({ played: s.played });
    }
    return [l, r];
  }

  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    for (let i = 0; i < left.length; i++) {
      let [l, r] = this.current === null ? [0, 0] : this.frame(this.current);
      if (this.fading) {
        // Equal-power: the new stream rises as the old one sinks.
        const t = this.fading.pos / this.fading.len;
        const gin = Math.sin((t * Math.PI) / 2), gout = Math.cos((t * Math.PI) / 2);
        const [ol, or] = this.frame(this.fading.from);
        l = l * gin + ol * gout;
        r = r * gin + or * gout;
        if (++this.fading.pos >= this.fading.len) {
          this.port.postMessage({ faded: this.fading.from });
          this.fading = null;
        }
      }
      left[i] = l;
      right[i] = r;
    }
    this.out += left.length;
    // Report the clock now and then, so the page can keep the visuals in step.
    if (this.out % 4096 < left.length) this.port.postMessage(this.out);
    return true;
  }
}

registerProcessor("shrinegen", ShrinePlayer);
