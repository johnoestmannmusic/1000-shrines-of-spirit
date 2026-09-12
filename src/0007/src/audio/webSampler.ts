import { clipDuration, audioClip, type AudioClip } from "@/core/dsp";
import {
  LOOKAHEAD_SEC,
  Scheduler,
  envelopeAt,
  loopChannel,
  region,
  samplePosition,
  waveform,
  type SamplerEvent,
  type SamplerSettings,
  type Sequence,
} from "@/core/sampler";
import { spectralRender, spectralWasmAvailable } from "@/core/spectral";
import type { SamplePlayhead } from "./backend";

export class Voice {
  released: { start: number; level: number; duration: number } | null = null;
  stolen = false;

  constructor(
    public source: AudioBufferSourceNode,
    public gain: GainNode,
    public pan: StereoPannerNode,
    public channel: number,
    public instrument: number,
    public settings: SamplerSettings,
    public level: number,
    public start: number,
    public end: number,
    public rate: number,
  ) {}

  levelAt(when: number): number {
    if (this.released && when >= this.released.start) {
      const { start, level, duration } = this.released;
      return level * Math.min(Math.max(1 - (when - start) / Math.max(duration, 0.0001), 0), 1);
    }
    return this.level * envelopeAt(this.settings, when - this.start);
  }

  release(when: number, fade: number): void {
    if (
      this.end <= when ||
      (this.released && this.released.start + this.released.duration <= when + fade)
    ) {
      return;
    }
    const level = this.levelAt(when);
    this.gain.gain.cancelScheduledValues(when);
    this.gain.gain.setValueAtTime(level, when);
    this.gain.gain.linearRampToValueAtTime(0, when + fade);
    try {
      this.source.stop(when + fade + 0.02);
    } catch {
      /* already stopped */
    }
    this.released = { start: when, level, duration: fade };
    this.end = Math.min(this.end, when + fade + 0.02);
  }

  position(now: number): number {
    return samplePosition(this.settings, now - this.start, this.rate);
  }

  playhead(now: number): SamplePlayhead | null {
    if (!(this.start <= now && now < this.end)) return null;
    if (this.settings.sourceIndex === null) return null;
    return {
      source: this.settings.sourceIndex,
      instrument: this.instrument,
      position: this.position(now),
      fused: this.settings.spectral.enabled,
      level: Math.min(Math.max(this.levelAt(now), 0.15), 1),
    };
  }

  pitchRamp(baseRate: number, when: number, duration: number): void {
    const target = baseRate * Math.pow(2, Math.min(Math.max(this.settings.transpose, -48), 48) / 12);
    this.source.playbackRate.setValueAtTime(this.rate, when);
    this.source.playbackRate.linearRampToValueAtTime(target, when + Math.max(duration, 0.0001));
    if (!this.settings.looping) {
      const regionLen = Math.max(this.settings.endSec - this.settings.startSec, 0);
      this.end = Math.max(this.end, when + regionLen / Math.max(Math.min(this.rate, target), 0.001));
    }
    this.rate = target;
  }

  dispose(): void {
    try {
      this.source.stop();
    } catch {
      /* already stopped */
    }
    try {
      this.source.disconnect();
      this.gain.disconnect();
      this.pan.disconnect();
    } catch {
      /* already disconnected */
    }
  }
}

export function buildVoice(
  ctx: AudioContext,
  buffer: AudioBuffer,
  settings: SamplerSettings,
  instrument: number,
  channel: number,
  baseRate: number,
  volume: number,
  when: number,
  destination: AudioNode,
): Voice {
  const source = ctx.createBufferSource();
  const gain = ctx.createGain();
  const pan = ctx.createStereoPanner();

  const rate = baseRate * Math.pow(2, Math.min(Math.max(settings.transpose, -48), 48) / 12);
  const level = volume * Math.min(Math.max(settings.volume, 0), 1.5);
  const attack = Math.min(Math.max(settings.attack, 0.003), 5);
  const decay = Math.min(Math.max(settings.decay, 0), 5);

  source.buffer = buffer;
  source.playbackRate.value = rate;

  gain.gain.setValueAtTime(0, when);
  gain.gain.linearRampToValueAtTime(level, when + attack);
  gain.gain.linearRampToValueAtTime(
    level * Math.min(Math.max(settings.sustain, 0), 1),
    when + attack + decay,
  );

  const panCentre = Math.min(Math.max(settings.pan, -1), 1);
  const panWidth = Math.min(Math.max(settings.panRandomRange, 0), 1);
  pan.pan.value = Math.min(Math.max(panCentre + (Math.random() * 2 - 1) * panWidth, -1), 1);

  source.connect(gain);
  gain.connect(pan);
  pan.connect(destination);

  let end: number;
  if (settings.looping) {
    source.loop = true;
    source.loopStart = 0;
    source.loopEnd = buffer.duration;
    source.start(when);
    end = Number.POSITIVE_INFINITY;
  } else {
    const reg = region(settings, buffer.duration);
    if (!reg) throw new Error("Empty trim");
    const [offset, length] = reg;
    source.start(when, offset, length);
    const playDuration = length / rate;
    end = when + playDuration;
    // Short fade at the end so a one-shot that ends on a non-zero sample
    // doesn't click/pop.
    const fade = Math.min(0.005, playDuration / 2);
    if (fade > 0) {
      const envEnd = envelopeAt(settings, Math.max(playDuration - fade, 0));
      gain.gain.setValueAtTime(level * envEnd, end - fade);
      gain.gain.linearRampToValueAtTime(0, end);
    }
  }

  return new Voice(source, gain, pan, channel, instrument, settings, level, when, end, rate);
}

export interface LoopCache {
  source: number;
  start: number;
  end: number;
  pingPong: boolean;
  buffer: AudioBuffer;
}

/** Decoded + rendered sample state and the look-ahead sampler voice engine. */
export class SamplerEngine {
  sequence: Sequence | null = null;
  settings: SamplerSettings[] = [];
  samples: Array<AudioBuffer | null> = [];
  clips: Array<AudioClip | null> = [];
  waveforms: Array<Array<[number, number]>> = [];
  fused: Array<AudioBuffer | null> = [];
  fusedClips: Array<AudioClip | null> = [];
  fusedWaveforms: Array<Array<[number, number]>> = [];
  ready = false;
  error: string | null = null;
  loops: Array<LoopCache | null> = [];
  scheduler: Scheduler | null = null;
  voices: Voice[] = [];
  rendering: boolean[] = [];
  private fusionJustCompleted: boolean[] = [];
  private renderGeneration: number[] = [];

  setSequence(sequence: Sequence): void {
    this.sequence = sequence;
  }

  setSettingsVec(settings: SamplerSettings[]): void {
    this.settings = settings;
  }

  updateSettings(index: number, settings: SamplerSettings, now: number): void {
    if (settings.muted || settings.sourceIndex === null) {
      for (const voice of this.voices) {
        if (voice.instrument === index && voice.end > now) voice.release(now, 0.008);
      }
    }
    this.settings[index] = settings;
  }

  async decodeAll(ctx: AudioContext, sampleBytes: Array<Uint8Array | null>): Promise<void> {
    this.samples = [];
    this.clips = [];
    this.waveforms = [];
    this.fused = [];
    this.fusedClips = [];
    this.fusedWaveforms = [];
    this.loops = [];
    this.ready = false;
    try {
      for (const bytes of sampleBytes) {
        if (!bytes || bytes.length === 0) {
          this.samples.push(null);
          this.clips.push(null);
          this.waveforms.push([]);
          continue;
        }
        const arrayBuffer = bytes.buffer.slice(
          bytes.byteOffset,
          bytes.byteOffset + bytes.byteLength,
        ) as ArrayBuffer;
        const audioBuffer = await ctx.decodeAudioData(arrayBuffer);
        this.samples.push(audioBuffer);
        const channelData = audioBuffer.getChannelData(0);
        this.waveforms.push(waveform(channelData, 600));
        const channels: Float32Array[] = [];
        for (let c = 0; c < audioBuffer.numberOfChannels; c++) {
          channels.push(new Float32Array(audioBuffer.getChannelData(c)));
        }
        this.clips.push(audioClip(channels, audioBuffer.sampleRate));
      }
      this.ensureSize(this.settings.length);
      if (spectralWasmAvailable()) {
        const renders: Promise<void>[] = [];
        for (let i = 0; i < this.settings.length; i++) {
          const s = this.settings[i]!;
          if (
            s.spectral.enabled &&
            s.sourceIndex !== null &&
            (!this.spectralNeedsB(i) || s.spectral.sourceIndex2 !== null)
          ) {
            renders.push(this.renderSpectral(ctx, i));
          }
        }
        await Promise.allSettled(renders);
      }
      this.ready = true;
    } catch (e) {
      this.error = `Cannot decode source samples: ${String(e)}`;
    }
  }

  private spectralNeedsB(index: number): boolean {
    return this.settings[index]!.spectral.mode !== "off";
  }

  private ensureSize(count: number): void {
    for (let i = this.fused.length; i < count; i++) {
      this.fused.push(null);
      this.fusedClips.push(null);
      this.fusedWaveforms.push([]);
      this.loops.push(null);
      this.rendering.push(false);
      this.fusionJustCompleted.push(false);
      this.renderGeneration.push(0);
    }
  }

  /** Reads and clears the one-shot "a render just finished" flag for an instrument. */
  takeFusionCompleted(instrument: number): boolean {
    const completed = this.fusionJustCompleted[instrument] ?? false;
    this.fusionJustCompleted[instrument] = false;
    return completed;
  }

  async renderSpectral(ctx: AudioContext, instrument: number): Promise<void> {
    const s = this.settings[instrument];
    if (!s) return;
    this.ensureSize(instrument + 1);
    const generation = ++this.renderGeneration[instrument]!;
    this.fused[instrument] = null;
    this.fusedClips[instrument] = null;
    this.fusedWaveforms[instrument] = [];
    this.loops[instrument] = null;
    if (!s.spectral.enabled) return;
    this.rendering[instrument] = true;
    try {
      const source = s.sourceIndex;
      if (source === null) throw new Error("Sample A is required");
      const a = this.clips[source];
      if (!a) throw new Error("Sample A is not ready");
      const b =
        s.spectral.sourceIndex2 !== null ? this.clips[s.spectral.sourceIndex2] ?? null : null;
      const clip = await spectralRender(a, b, s.spectral);
      if (this.renderGeneration[instrument] !== generation) return; // superseded by a newer render
      const buffer = ctx.createBuffer(
        clip.channels.length,
        clip.channels[0]!.length,
        clip.sampleRate,
      );
      for (let c = 0; c < clip.channels.length; c++) {
        buffer.getChannelData(c).set(clip.channels[c]!);
      }
      this.fused[instrument] = buffer;
      this.fusedClips[instrument] = clip;
      this.fusedWaveforms[instrument] = waveform(clip.channels[0]!, 600);
      this.fusionJustCompleted[instrument] = true;
    } catch (e) {
      if (this.renderGeneration[instrument] !== generation) return; // superseded, ignore its error too
      this.error = `Cannot render Spectral instrument: ${String(e)}`;
    } finally {
      if (this.renderGeneration[instrument] === generation) this.rendering[instrument] = false;
    }
  }

  private spectralActive(instrument: number): boolean {
    const s = this.settings[instrument];
    return !!s && s.spectral.enabled && spectralWasmAvailable();
  }

  buffer(ctx: AudioContext, instrument: number): AudioBuffer {
    const settings = this.settings[instrument];
    if (!settings) throw new Error("No instrument settings");
    const source = settings.sourceIndex;
    if (source === null) throw new Error("Sample is not assigned");
    const original = this.spectralActive(instrument)
      ? this.fused[instrument]
      : this.samples[source];
    if (!original) throw new Error("Sample is not ready");
    const reg = region(settings, original.duration);
    if (!reg) throw new Error("Empty trim");
    const [start, length] = reg;
    if (!settings.looping) return original;

    const cache = this.loops[instrument];
    if (
      cache &&
      cache.source === source &&
      cache.start === start &&
      cache.end === start + length &&
      cache.pingPong === settings.pingPong
    ) {
      return cache.buffer;
    }

    const channels: Float32Array[] = [];
    for (let c = 0; c < original.numberOfChannels; c++) {
      channels.push(
        loopChannel(original.getChannelData(c), original.sampleRate, start, start + length, settings.pingPong),
      );
    }
    const buffer = ctx.createBuffer(
      original.numberOfChannels,
      channels[0]!.length,
      original.sampleRate,
    );
    for (let c = 0; c < channels.length; c++) buffer.getChannelData(c).set(channels[c]!);
    this.loops[instrument] = {
      source,
      start,
      end: start + length,
      pingPong: settings.pingPong,
      buffer,
    };
    return buffer;
  }

  tick(ctx: AudioContext, channelGains: Array<GainNode | null>): void {
    const now = ctx.currentTime;
    for (let i = this.voices.length - 1; i >= 0; i--) {
      const voice = this.voices[i]!;
      if (voice.end <= now) {
        voice.dispose();
        this.voices.splice(i, 1);
      }
    }
    const scheduler = this.scheduler;
    const sequence = this.sequence;
    if (!scheduler || !sequence) return;

    for (const scheduled of scheduler.tick(now, LOOKAHEAD_SEC)) {
      const events = sequence.rows[scheduled.row] ?? [];
      for (const event of events) {
        this.handleEvent(ctx, channelGains, event, scheduled.when, now);
      }
    }
  }

  private findLastVoice(channel: number, when: number): Voice | undefined {
    for (let i = this.voices.length - 1; i >= 0; i--) {
      const voice = this.voices[i]!;
      if (voice.channel === channel && voice.end > when && !voice.stolen) return voice;
    }
    return undefined;
  }

  private handleEvent(
    ctx: AudioContext,
    channelGains: Array<GainNode | null>,
    event: SamplerEvent,
    when: number,
    now: number,
  ): void {
    if (event.type === "off") {
      const voice = this.findLastVoice(event.channel, when);
      if (voice) voice.release(when, Math.min(Math.max(voice.settings.release, 0), 5));
      return;
    }
    if (event.type === "pitchRamp") {
      const voice = this.findLastVoice(event.channel, when);
      if (voice) voice.pitchRamp(event.rate, when, event.duration);
      return;
    }

    const settings = this.settings[event.instrument];
    if (!settings || settings.muted || settings.sourceIndex === null) return;
    let buffer: AudioBuffer;
    try {
      buffer = this.buffer(ctx, event.instrument);
    } catch (e) {
      this.error = `Cannot play sample: ${String(e)}`;
      return;
    }

    if (settings.polyphonic) {
      const cap = Math.min(Math.max(settings.voiceCap, 1), 32);
      const active = () =>
        this.voices.filter(
          (v) => v.instrument === event.instrument && v.end > when && !v.stolen,
        );
      while (active().length >= cap) {
        const victim = active()[0];
        if (!victim) break;
        victim.release(when, 0.008);
        victim.stolen = true;
      }
    } else {
      const victim = this.findLastVoice(event.channel, when);
      if (victim) {
        victim.release(when, 0.008);
        victim.stolen = true;
      }
    }

    const destination = channelGains[event.channel] ?? ctx.destination;
    try {
      const voice = buildVoice(
        ctx,
        buffer,
        settings,
        event.instrument,
        event.channel,
        event.rate,
        event.volume,
        when,
        destination,
      );
      this.voices.push(voice);
    } catch (e) {
      this.error = `Cannot play sample: ${String(e)}`;
    }
    void now;
  }

  playheads(now: number): SamplePlayhead[] {
    const out: SamplePlayhead[] = [];
    for (const voice of this.voices) {
      const ph = voice.playhead(now);
      if (ph) out.push(ph);
    }
    return out;
  }

  clear(): void {
    for (const voice of this.voices) voice.dispose();
    this.voices = [];
    this.scheduler = null;
  }

  effectiveWaveform(instrument: number): Array<[number, number]> {
    const s = this.settings[instrument];
    if (!s) return [];
    if (this.spectralActive(instrument)) return this.fusedWaveforms[instrument] ?? [];
    const src = s.sourceIndex;
    return src !== null ? this.waveforms[src] ?? [] : [];
  }

  effectiveClip(instrument: number): AudioClip | null {
    const s = this.settings[instrument];
    if (!s) return null;
    if (this.spectralActive(instrument)) return this.fusedClips[instrument] ?? null;
    const src = s.sourceIndex;
    return src !== null ? this.clips[src] ?? null : null;
  }

  effectiveDuration(instrument: number): number {
    const s = this.settings[instrument];
    if (!s) return 0;
    if (this.spectralActive(instrument)) {
      const buf = this.fused[instrument];
      return buf ? buf.duration : 0;
    }
    const src = s.sourceIndex;
    const buf = src !== null ? this.samples[src] : null;
    return buf ? buf.duration : 0;
  }

  /** Drops and rebuilds loop caches for all instruments. */
  invalidateLoops(): void {
    this.loops = this.loops.map(() => null);
  }

  clipsForPackaging(index: number): AudioClip | null {
    return this.clips[index] ?? null;
  }

  fusedReady(instrument: number): boolean {
    return this.fused[instrument] != null;
  }
}

export { clipDuration };
