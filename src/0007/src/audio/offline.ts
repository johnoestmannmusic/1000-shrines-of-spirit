import type { AudioClip } from "@/core/dsp";
import { clipLen } from "@/core/dsp";
import type { MasterFxSettings } from "@/core/masterFx";
import { createMasterFxGraph } from "./masterFxGraph";

const DEFAULT_RATE = 44_100;

/** Decodes encoded audio bytes (WAV/OGG/MP3) into an AudioClip at `sampleRate`. */
export async function decodeAudioBytes(
  bytes: Uint8Array,
  sampleRate = DEFAULT_RATE,
): Promise<AudioClip> {
  const ctx = new OfflineAudioContext(2, 1, sampleRate);
  const arrayBuffer = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
  const buffer = await ctx.decodeAudioData(arrayBuffer);
  const channels: Float32Array[] = [];
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    channels.push(new Float32Array(buffer.getChannelData(c)));
  }
  return { channels, sampleRate: buffer.sampleRate };
}

/** Extra seconds needed to render the delay/reverb tail in full. */
function fxTailSeconds(settings: MasterFxSettings): number {
  let tail = 0;
  const { delay, reverb } = settings;
  if (delay.enabled) {
    const feedback = Math.min(Math.max(delay.feedback, 0), 0.95);
    const repeats = feedback > 0 ? Math.log(0.001) / Math.log(feedback) : 1;
    tail += Math.min(delay.timeSec * repeats, 10);
  }
  if (reverb.enabled) tail += reverb.decaySec;
  return Math.min(tail, 15) + 0.05;
}

/**
 * Runs a decoded clip through the exact live Master FX chain (`createMasterFxGraph`)
 * on an `OfflineAudioContext`, capturing the delay/reverb tail.
 */
export async function applyMasterFxOffline(
  clip: AudioClip,
  settings: MasterFxSettings,
  onProgress?: (fraction: number) => void,
): Promise<AudioClip> {
  const frames = clipLen(clip);
  if (frames === 0 || (!settings.delay.enabled && !settings.reverb.enabled)) {
    onProgress?.(1);
    return clip;
  }

  const rate = clip.sampleRate;
  const tail = Math.ceil(fxTailSeconds(settings) * rate);
  const totalDuration = (frames + tail) / rate;
  const ctx = new OfflineAudioContext(2, frames + tail, rate);
  const graph = createMasterFxGraph(ctx, settings);

  const buffer = ctx.createBuffer(Math.max(1, clip.channels.length), frames, rate);
  clip.channels.forEach((channel, index) => buffer.getChannelData(index).set(channel));

  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.connect(graph.input);
  graph.output.connect(ctx.destination);
  source.start(0);

  // Precise progress via render checkpoints, but `suspend` is missing on some
  // browsers (e.g. Safari/Firefox builds) — fall back to a timed ramp there.
  const canCheckpoint =
    typeof ctx.suspend === "function" && typeof ctx.resume === "function";
  let timer: ReturnType<typeof setInterval> | null = null;
  if (canCheckpoint) {
    const steps = 24;
    for (let i = 1; i < steps; i++) {
      const time = (i / steps) * totalDuration;
      void ctx
        .suspend(time)
        .then(() => {
          onProgress?.(i / steps);
          void ctx.resume();
        })
        .catch(() => {
          /* checkpoint unsupported/failed — ignore */
        });
    }
  } else if (onProgress) {
    const started = Date.now();
    timer = setInterval(() => {
      const elapsed = (Date.now() - started) / 1000;
      onProgress(0.9 * (1 - Math.exp(-elapsed / 2)));
    }, 100);
  }

  let rendered: AudioBuffer;
  try {
    rendered = await ctx.startRendering();
  } finally {
    if (timer !== null) clearInterval(timer);
  }
  onProgress?.(1);
  const channels: Float32Array[] = [];
  for (let c = 0; c < rendered.numberOfChannels; c++) {
    channels.push(new Float32Array(rendered.getChannelData(c)));
  }
  return { channels, sampleRate: rendered.sampleRate };
}
