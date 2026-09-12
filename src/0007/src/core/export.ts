import { audioClip, clipDuration, clipIsEmpty, clipLen, type AudioClip } from "./dsp";
import {
  envelopeAt,
  region,
  sequenceDuration,
  type SamplerSettings,
  type Sequence,
} from "./sampler";

function pushU16(out: number[], value: number): void {
  out.push(value & 0xff, (value >>> 8) & 0xff);
}

function pushU32(out: number[], value: number): void {
  out.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
}

/** Encodes an AudioClip as a canonical 16-bit PCM WAV file. */
export function wavPcm16(clip: AudioClip): Uint8Array {
  const channels = clip.channels.length;
  if (clipIsEmpty(clip)) throw new Error("Audio clip is empty");
  if (channels > 65535) throw new Error("Too many channels for a WAV file");
  const blockAlign = channels * 2;
  const frames = clipLen(clip);
  const dataSize = frames * blockAlign;
  if (dataSize > 0xffffffff - 36) throw new Error("Audio is too large for a PCM WAV file");

  const out: number[] = [];
  const writeAscii = (s: string) => {
    for (const ch of s) out.push(ch.charCodeAt(0));
  };

  writeAscii("RIFF");
  pushU32(out, 36 + dataSize);
  writeAscii("WAVEfmt ");
  pushU32(out, 16);
  pushU16(out, 1);
  pushU16(out, channels);
  pushU32(out, clip.sampleRate);
  pushU32(out, clip.sampleRate * blockAlign);
  pushU16(out, blockAlign);
  pushU16(out, 16);
  writeAscii("data");
  pushU32(out, dataSize);

  for (let frame = 0; frame < frames; frame++) {
    for (let c = 0; c < channels; c++) {
      const raw = clip.channels[c]![frame]!;
      const sample = Math.min(Math.max(raw, -1), 1);
      const value = sample < 0 ? Math.trunc(sample * 32768) : Math.trunc(sample * 32767);
      pushU16(out, value & 0xffff);
    }
  }

  return Uint8Array.from(out);
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

export function crc32(data: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < data.length; i++) c = CRC_TABLE[(c ^ data[i]!) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export interface ZipEntry {
  name: string;
  data: Uint8Array;
}

/** Minimal STORE-method (uncompressed) ZIP writer, matching lantern-core. */
export function zipStore(entries: ZipEntry[]): Uint8Array {
  if (entries.length > 65535) throw new Error("Too many ZIP entries");
  const encoder = new TextEncoder();
  const local: number[] = [];
  const central: number[] = [];

  const u16 = (out: number[], v: number) => pushU16(out, v);
  const u32 = (out: number[], v: number) => pushU32(out, v);

  for (const entry of entries) {
    const nameBytes = encoder.encode(entry.name);
    if (entry.name.length === 0 || nameBytes.length > 65535) {
      throw new Error("Invalid ZIP entry name");
    }
    for (const byte of nameBytes) {
      if (byte > 0x7f) throw new Error("ZIP entry names must be ASCII");
    }
    const crc = crc32(entry.data);
    const size = entry.data.length;
    const offset = local.length;

    u32(local, 0x04034b50);
    u16(local, 20);
    u16(local, 0);
    u16(local, 0);
    u16(local, 0);
    u16(local, 0x0021);
    u32(local, crc);
    u32(local, size);
    u32(local, size);
    u16(local, nameBytes.length);
    u16(local, 0);
    for (const b of nameBytes) local.push(b);
    for (const b of entry.data) local.push(b);

    u32(central, 0x02014b50);
    u16(central, 20);
    u16(central, 20);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0x0021);
    u32(central, crc);
    u32(central, size);
    u32(central, size);
    u16(central, nameBytes.length);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0);
    u32(central, 0);
    u32(central, offset);
    for (const b of nameBytes) central.push(b);
  }

  const centralOffset = local.length;
  const all = local.concat(central);
  u32(all, 0x06054b50);
  u16(all, 0);
  u16(all, 0);
  u16(all, entries.length);
  u16(all, entries.length);
  u32(all, central.length);
  u32(all, centralOffset);
  u16(all, 0);

  return Uint8Array.from(all);
}

// ---- Offline sampler mixdown (ports lantern-core::export::render_sampler_mix) ----

const OUTPUT_RATE = 44_100;
const RENDER_PRNG_SEED = 0x4c414e5445524e01n;

interface RateRamp {
  start: number;
  end: number;
  from: number;
  to: number;
}

interface RenderVoice {
  instrument: number;
  channel: number;
  start: number;
  end: number;
  release: { start: number; level: number; duration: number } | null;
  rate: number;
  rateRamps: RateRamp[];
  level: number;
  pan: number;
  settings: SamplerSettings;
  clip: AudioClip;
  stolen: boolean;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function mod(value: number, m: number): number {
  return ((value % m) + m) % m;
}

function rateAt(voice: RenderVoice, time: number): number {
  let current = voice.rate;
  for (const ramp of voice.rateRamps) {
    if (time < ramp.start) return current;
    if (time <= ramp.end) {
      const amount = (time - ramp.start) / Math.max(ramp.end - ramp.start, 0.0001);
      return ramp.from + (ramp.to - ramp.from) * amount;
    }
    current = ramp.to;
  }
  return current;
}

function voiceEnvelope(voice: RenderVoice, time: number): number {
  if (voice.release && time >= voice.release.start) {
    const { start, level, duration } = voice.release;
    return level * clamp(1 - (time - start) / Math.max(duration, 0.0001), 0, 1);
  }
  return voice.level * envelopeAt(voice.settings, time - voice.start);
}

function releaseVoice(voice: RenderVoice, time: number, duration: number, stolen: boolean): void {
  if (voice.end <= time) return;
  const level = voiceEnvelope(voice, time);
  voice.release = { start: time, level, duration };
  voice.end = Math.min(voice.end, time + duration + 0.02);
  voice.stolen = voice.stolen || stolen;
}

function makePrng(seed: bigint): () => number {
  let state = seed & 0xffffffffffffffffn;
  return () => {
    state ^= (state << 13n) & 0xffffffffffffffffn;
    state ^= state >> 7n;
    state ^= (state << 17n) & 0xffffffffffffffffn;
    state &= 0xffffffffffffffffn;
    return Number((state >> 32n) & 0xffffffffn);
  };
}

/**
 * Renders the current sampler sequence offline to a stereo AudioClip,
 * applying per-instrument trim/loop/ADSR/transpose/pan, stage release and
 * channel/master mix — the same semantics as live Web Audio playback.
 */
export function renderSamplerMix(
  sequence: Sequence,
  settings: SamplerSettings[],
  clips: Array<AudioClip | null>,
  channelVolume: number[],
  channelMuted: boolean[],
  master: number,
): AudioClip {
  const duration = sequenceDuration(sequence);
  const voices: RenderVoice[] = [];
  const lastByChannel: Array<RenderVoice | null> = [null, null, null, null];
  const random = makePrng(RENDER_PRNG_SEED);

  for (let row = 0; row < sequence.rows.length; row++) {
    const time = sequence.rowTimes[row] ?? 0;
    for (const event of sequence.rows[row] ?? []) {
      if (event.type === "off") {
        const voice = lastByChannel[event.channel];
        if (voice) releaseVoice(voice, time, clamp(voice.settings.release, 0, 5), false);
        continue;
      }
      if (event.type === "pitchRamp") {
        const voice = lastByChannel[event.channel];
        if (voice && voice.end > time && !voice.stolen) {
          const from = rateAt(voice, time);
          const to =
            event.rate * Math.pow(2, clamp(voice.settings.transpose, -48, 48) / 12);
          voice.rateRamps.push({ start: time, end: time + event.duration, from, to });
        }
        continue;
      }

      const setting = settings[event.instrument];
      const clip = clips[event.instrument];
      if (!setting || !clip) continue;
      if (setting.muted || setting.sourceIndex === null) continue;
      if (!region(setting, clipDuration(clip))) continue;

      if (setting.polyphonic) {
        const cap = Math.floor(clamp(setting.voiceCap, 1, 32));
        const active = () =>
          voices.filter(
            (v) => v.instrument === event.instrument && v.end > time && !v.stolen,
          );
        while (active().length >= cap) {
          const victim = active()[0];
          if (!victim) break;
          releaseVoice(victim, time, 0.008, true);
        }
      } else {
        const previous = lastByChannel[event.channel];
        if (previous && previous.end > time && !previous.stolen) {
          releaseVoice(previous, time, 0.008, true);
        }
      }

      const panCentre = clamp(setting.pan, -1, 1);
      const panWidth = clamp(setting.panRandomRange, 0, 1);
      const pan = clamp(panCentre + ((random() / 0xffffffff) * 2 - 1) * panWidth, -1, 1);
      const rate = event.rate * Math.pow(2, clamp(setting.transpose, -48, 48) / 12);
      const voice: RenderVoice = {
        instrument: event.instrument,
        channel: event.channel,
        start: time,
        end: duration,
        release: null,
        rate,
        rateRamps: [],
        level: event.volume * clamp(setting.volume, 0, 1.5),
        pan,
        settings: setting,
        clip,
        stolen: false,
      };
      voices.push(voice);
      lastByChannel[event.channel] = voice;
    }
  }

  const frames = Math.ceil(duration * OUTPUT_RATE) + 1;
  const left = new Float32Array(frames);
  const right = new Float32Array(frames);

  for (const voice of voices) {
    const reg = region(voice.settings, clipDuration(voice.clip));
    if (!reg) continue;
    const [regionStart, regionLen] = reg;
    const startFrame = Math.floor(voice.start * OUTPUT_RATE);
    const endFrame = Math.min(Math.ceil(Math.min(voice.end, duration) * OUTPUT_RATE), frames);
    const mix = channelMuted[voice.channel] ? 0 : (channelVolume[voice.channel] ?? 1) * master;
    const angle = ((voice.pan + 1) * Math.PI) / 4;
    const mono = voice.clip.channels.length === 1;
    const regionChannel = (ch: number) =>
      voice.clip.channels[Math.min(ch, voice.clip.channels.length - 1)]!;

    let traveled = 0;
    let previousTime = voice.start;
    let previousRate = rateAt(voice, voice.start);

    for (let frame = startFrame; frame < endFrame; frame++) {
      const t = frame / OUTPUT_RATE;
      const rate = rateAt(voice, t);
      traveled += (t - previousTime) * ((rate + previousRate) / 2);
      previousTime = t;
      previousRate = rate;

      let rel: number;
      if (voice.settings.looping && voice.settings.pingPong) {
        const phase = mod(traveled, 2 * regionLen);
        rel = phase <= regionLen ? phase : 2 * regionLen - phase;
      } else if (voice.settings.looping) {
        rel = mod(traveled, regionLen);
      } else if (traveled < regionLen) {
        rel = traveled;
      } else {
        break;
      }

      const sourceFrame = (regionStart + rel) * voice.clip.sampleRate;
      const i0 = Math.floor(sourceFrame);
      const frac = sourceFrame - i0;
      const sample = (ch: number): number => {
        const data = regionChannel(ch);
        const a = data[i0] ?? 0;
        const b = data[i0 + 1] ?? a;
        return a + (b - a) * frac;
      };

      const fade = Math.max(Math.min(0.005, regionLen / 2), 1e-9);
      const edge = clamp(Math.min(rel / fade, (regionLen - rel) / fade), 0, 1);
      const gain = voiceEnvelope(voice, t) * mix * edge;

      if (mono) {
        const s = sample(0);
        left[frame] = left[frame]! + s * Math.cos(angle) * gain;
        right[frame] = right[frame]! + s * Math.sin(angle) * gain;
      } else {
        left[frame] = left[frame]! + sample(0) * (1 - Math.max(voice.pan, 0)) * gain;
        right[frame] = right[frame]! + sample(1) * (1 + Math.min(voice.pan, 0)) * gain;
      }
    }
  }

  return audioClip([left, right], OUTPUT_RATE);
}
