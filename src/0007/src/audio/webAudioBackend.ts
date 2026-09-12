import type { AudioClip } from "@/core/dsp";
import { clipDuration } from "@/core/dsp";
import { Scheduler, waveform, type SamplerSettings, type Sequence } from "@/core/sampler";
import {
  NUM_CHANNELS,
  type AudioBackend,
  type PatternNote,
  type PlaybackMode,
  type SamplePlayhead,
} from "./backend";
import { SamplerEngine, Voice, buildVoice } from "./webSampler";

async function decodeBytes(ctx: AudioContext, bytes: Uint8Array): Promise<AudioBuffer> {
  const arrayBuffer = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
  return ctx.decodeAudioData(arrayBuffer);
}

// Instrument previews audition at C5 (three semitones above the A4/440
// reference the samples are assumed to be at), rather than at A4 itself.
const PREVIEW_RATE = Math.pow(2, 3 / 12);

interface InstrumentPreview {
  voice: Voice;
  tone: OscillatorNode | null;
  toneGain: GainNode | null;
}

export class WebAudioBackend implements AudioBackend {
  private ctx: AudioContext | null = null;
  private masterGain: GainNode | null = null;
  private masterAnalyser: AnalyserNode | null = null;
  private channelGain: Array<GainNode | null> = [null, null, null, null];
  private channelAnalyser: Array<AnalyserNode | null> = [null, null, null, null];
  private meterBuffer = new Float32Array(512);

  private channelVolume = [1, 1, 1, 1];
  private channelMuted = [false, false, false, false];
  private masterVolume = 1;

  private mode: PlaybackMode = "chip";
  private tuning = 440;
  private webError: string | null = null;

  private sampler = new SamplerEngine();
  private timer: ReturnType<typeof setInterval> | null = null;

  private instrumentPreview: InstrumentPreview | null = null;
  private sourcePreview: {
    node: AudioBufferSourceNode;
    gain: GainNode;
    source: number;
    start: number;
    duration: number;
  } | null = null;
  private patternStemPreview: AudioBufferSourceNode[] = [];
  private patternSamplerPreview: Voice[] = [];

  private stemBuffers: Array<AudioBuffer | null> = [null, null, null, null];
  private stemsReadyFlag = false;
  private chipDuration = 0;
  private stemSources: Array<AudioBufferSourceNode | null> = [null, null, null, null];

  private songStartOffset = 0;
  private songStartCtxTime = 0;
  private songStarted = false;

  private sequence: Sequence | null = null;

  ensureStarted(): void {
    if (this.ctx) return;
    const ctx = new AudioContext();
    const master = ctx.createGain();
    const masterAnalyser = ctx.createAnalyser();
    masterAnalyser.fftSize = 512;
    master.gain.value = this.masterVolume;
    master.connect(masterAnalyser);
    masterAnalyser.connect(ctx.destination);
    for (let c = 0; c < NUM_CHANNELS; c++) {
      const gain = ctx.createGain();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 512;
      gain.gain.value = this.channelMuted[c] ? 0 : this.channelVolume[c]!;
      gain.connect(analyser);
      analyser.connect(master);
      this.channelGain[c] = gain;
      this.channelAnalyser[c] = analyser;
    }
    this.ctx = ctx;
    this.masterGain = master;
    this.masterAnalyser = masterAnalyser;
  }

  loadSampler(
    sequence: Sequence,
    settings: SamplerSettings[],
    samples: Array<Uint8Array | null>,
  ): void {
    this.tuning = sequence.tuning;
    this.sequence = sequence;
    this.ensureStarted();
    const ctx = this.ctx!;
    this.sampler.setSequence(sequence);
    this.sampler.setSettingsVec(settings);
    this.sampler.decodeAll(ctx, samples).catch((e) => {
      this.webError = `Cannot decode source samples: ${String(e)}`;
    });
  }

  updateSequence(sequence: Sequence): void {
    this.sequence = sequence;
    this.sampler.setSequence(sequence);
  }

  replaceSample(source: number, bytes: Uint8Array | null): void {
    this.stopSamplePreview();
    if (!this.ctx) this.ensureStarted();
    const ctx = this.ctx!;
    void (async () => {
      try {
        if (!bytes || bytes.length === 0) {
          this.sampler.samples[source] = null;
          this.sampler.clips[source] = null;
          this.sampler.waveforms[source] = [];
        } else {
          const buffer = await decodeBytes(ctx, bytes);
          const channelData = buffer.getChannelData(0);
          const channels: Float32Array[] = [];
          for (let c = 0; c < buffer.numberOfChannels; c++) {
            channels.push(new Float32Array(buffer.getChannelData(c)));
          }
          this.sampler.samples[source] = buffer;
          this.sampler.clips[source] = {
            channels,
            sampleRate: buffer.sampleRate,
          };
          this.sampler.waveforms[source] = waveform(channelData, 600);
        }
        this.sampler.invalidateLoops();
        for (let i = 0; i < this.sampler.settings.length; i++) {
          const s = this.sampler.settings[i]!;
          if (s.sourceIndex === source || s.spectral.sourceIndex2 === source) {
            this.sampler.fused[i] = null;
            this.sampler.fusedClips[i] = null;
            this.sampler.fusedWaveforms[i] = [];
            if (s.spectral.enabled && s.sourceIndex !== null) await this.sampler.renderSpectral(ctx, i);
          }
        }
      } catch (e) {
        this.webError = `Cannot decode sample: ${String(e)}`;
      }
    })();
  }

  previewSample(source: number): void {
    this.stopSamplePreview();
    this.ensureStarted();
    const ctx = this.ctx!;
    const buffer = this.sampler.samples[source];
    if (!buffer || !this.masterGain) return;
    const node = ctx.createBufferSource();
    const gain = ctx.createGain();
    node.buffer = buffer;
    node.connect(gain);
    gain.connect(this.masterGain);
    node.start();
    this.sourcePreview = {
      node,
      gain,
      source,
      start: ctx.currentTime,
      duration: buffer.duration,
    };
  }

  stopSamplePreview(): void {
    const preview = this.sourcePreview;
    if (!preview) return;
    this.sourcePreview = null;
    const ctx = this.ctx;
    if (ctx) {
      const now = ctx.currentTime;
      try {
        preview.gain.gain.cancelScheduledValues(now);
        preview.gain.gain.setValueAtTime(preview.gain.gain.value, now);
        preview.gain.gain.linearRampToValueAtTime(0, now + 0.005);
      } catch {
        /* ignore */
      }
      setTimeout(() => {
        try {
          preview.node.stop();
          preview.node.disconnect();
          preview.gain.disconnect();
        } catch {
          /* already stopped */
        }
      }, 12);
    } else {
      try {
        preview.node.stop();
        preview.node.disconnect();
      } catch {
        /* already stopped */
      }
    }
  }

  samplePlayheads(): SamplePlayhead[] {
    if (!this.ctx) return [];
    const now = this.ctx.currentTime;
    const out = this.sampler.playheads(now).concat(this.patternSamplerPreview.map((v) => v.playhead(now)).filter((p): p is SamplePlayhead => p !== null));
    const preview = this.instrumentPreview?.voice.playhead(now);
    if (preview) out.push(preview);
    if (this.sourcePreview) {
      const position = now - this.sourcePreview.start;
      if (position >= 0 && position <= this.sourcePreview.duration) {
        out.push({
          source: this.sourcePreview.source,
          instrument: null,
          position,
          fused: false,
          level: 1,
        });
      }
    }
    return out;
  }

  sampleWaveform(source: number): Array<[number, number]> {
    return this.sampler.waveforms[source] ?? [];
  }

  sampleClip(source: number): AudioClip | null {
    return this.sampler.clips[source] ?? null;
  }

  effectiveClip(instrument: number): AudioClip | null {
    return this.sampler.effectiveClip(instrument);
  }

  effectiveWaveform(instrument: number): Array<[number, number]> {
    return this.sampler.effectiveWaveform(instrument);
  }

  fusionWaveform(instrument: number): Array<[number, number]> {
    return this.sampler.fusedWaveforms[instrument] ?? [];
  }

  effectiveDuration(instrument: number): number {
    return this.sampler.effectiveDuration(instrument);
  }

  fusionReady(instrument: number): boolean {
    return this.sampler.fusedReady(instrument);
  }

  fusionRendering(instrument: number): boolean {
    return this.sampler.rendering[instrument] ?? false;
  }

  renderFusion(instrument: number): boolean {
    if (!this.ctx) return false;
    void this.sampler.renderSpectral(this.ctx, instrument);
    return true;
  }

  takeFusionCompleted(instrument: number): boolean {
    return this.sampler.takeFusionCompleted(instrument);
  }

  preview(instrument: number, reference: boolean): void {
    this.stopPreview();
    this.ensureStarted();
    const ctx = this.ctx!;
    void ctx.resume();
    let buffer: AudioBuffer;
    try {
      buffer = this.sampler.buffer(ctx, instrument);
    } catch (e) {
      this.webError = `Cannot preview: ${String(e)}`;
      return;
    }
    const settings = this.sampler.settings[instrument];
    if (!settings) return;
    let voice: Voice;
    try {
      voice = buildVoice(
        ctx,
        buffer,
        settings,
        instrument,
        0,
        PREVIEW_RATE,
        settings.volume,
        ctx.currentTime + 0.01,
        ctx.destination,
      );
    } catch (e) {
      this.webError = `Cannot preview: ${String(e)}`;
      return;
    }
    if (settings.looping) {
      voice.release(voice.start + 1.5, Math.min(Math.max(settings.release, 0), 5));
    }

    let tone: OscillatorNode | null = null;
    let toneGain: GainNode | null = null;
    if (reference) {
      tone = ctx.createOscillator();
      toneGain = ctx.createGain();
      tone.frequency.value = this.tuning * PREVIEW_RATE;
      const start = voice.start;
      const end = Math.min(voice.end, start + 5);
      const fade = Math.min(0.01, (end - start) / 4);
      const level = Math.pow(10, -8 / 20);
      toneGain.gain.setValueAtTime(0, start);
      toneGain.gain.linearRampToValueAtTime(level, start + fade);
      toneGain.gain.setValueAtTime(level, end - fade);
      toneGain.gain.linearRampToValueAtTime(0, end);
      tone.connect(toneGain);
      toneGain.connect(ctx.destination);
      tone.start(start);
      tone.stop(end + 0.02);
    }
    this.instrumentPreview = { voice, tone, toneGain };
  }

  stopPreview(): void {
    const preview = this.instrumentPreview;
    if (!preview) return;
    preview.voice.dispose();
    if (preview.tone) {
      try {
        preview.tone.stop();
      } catch {
        /* ignore */
      }
      preview.tone.disconnect();
    }
    preview.toneGain?.disconnect();
    this.instrumentPreview = null;
  }

  previewPosition(): { instrument: number; position: number } | null {
    if (!this.instrumentPreview || !this.ctx) return null;
    const now = this.ctx.currentTime;
    const voice = this.instrumentPreview.voice;
    if (now >= voice.end) return null;
    return { instrument: voice.instrument, position: voice.position(now) };
  }

  previewPattern(
    channels: number[],
    offset: number,
    duration: number,
    notes: PatternNote[],
  ): void {
    this.stopPatternPreview();
    this.ensureStarted();
    const ctx = this.ctx!;
    void ctx.resume();
    const when = ctx.currentTime + 0.01;
    if (this.mode === "chip") {
      for (const channel of channels) {
        const buffer = this.stemBuffers[channel];
        if (!buffer) continue;
        const available = Math.max(buffer.duration - offset, 0);
        const playFor = Math.min(duration, available);
        if (playFor <= 0) continue;
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(ctx.destination);
        source.start(when, offset, playFor);
        this.patternStemPreview.push(source);
      }
      return;
    }
    for (const note of notes) {
      const settings = this.sampler.settings[note.instrument];
      if (!settings || settings.muted || settings.sourceIndex === null) continue;
      try {
        const buffer = this.sampler.buffer(ctx, note.instrument);
        const voice = buildVoice(
          ctx,
          buffer,
          settings,
          note.instrument,
          note.channel,
          note.rate,
          note.volume,
          when,
          ctx.destination,
        );
        voice.release(when + duration, Math.min(Math.max(settings.release, 0), 5));
        this.patternSamplerPreview.push(voice);
      } catch {
        // Audition is best-effort: a not-yet-ready sample/fusion render is not
        // an error worth surfacing (previously showed a stuck "Cannot audition").
      }
    }
  }

  stopPatternPreview(): void {
    for (const node of this.patternStemPreview) {
      try {
        node.stop();
        node.disconnect();
      } catch {
        /* ignore */
      }
    }
    this.patternStemPreview = [];
    for (const voice of this.patternSamplerPreview) voice.dispose();
    this.patternSamplerPreview = [];
  }

  samplerReady(): boolean {
    return this.sampler.ready;
  }

  sampleDurations(): number[] {
    return this.sampler.samples.map((b) => (b ? b.duration : 0));
  }

  setSamplerSettings(instrument: number, settings: SamplerSettings): void {
    this.sampler.updateSettings(instrument, settings, this.ctx?.currentTime ?? 0);
  }

  /** Replaces the whole per-instrument settings array (after add/delete/import). */
  replaceSettings(settings: SamplerSettings[]): void {
    this.sampler.setSettingsVec(settings);
    this.sampler.loops = settings.map(() => null);
  }

  setMode(mode: PlaybackMode): void {
    if (mode === this.mode) return;
    const wasPlaying = this.isPlaying();
    const position = this.currentTime();
    this.stopSources();
    this.songStarted = false;
    this.mode = mode;
    this.songStartOffset = position;
    if (wasPlaying) this.play(this.songStartOffset);
  }

  error(): string | null {
    return this.webError ?? this.sampler.error;
  }

  loadStems(bytes: Array<Uint8Array | null>): void {
    this.ensureStarted();
    const ctx = this.ctx!;
    void (async () => {
      try {
        for (let i = 0; i < NUM_CHANNELS; i++) {
          const data = bytes[i];
          if (!data || data.length === 0) {
            this.stemBuffers[i] = null;
            continue;
          }
          const buffer = await decodeBytes(ctx, data);
          this.stemBuffers[i] = buffer;
          if (i === 0) this.chipDuration = buffer.duration;
        }
        this.stemsReadyFlag = true;
      } catch (e) {
        this.webError = `Cannot decode stem: ${String(e)}`;
      }
    })();
  }

  stemsReady(): boolean {
    return this.stemsReadyFlag;
  }

  songDuration(): number {
    if (this.mode === "sampler") {
      return this.sequence ? (this.sequence.rowTimes[this.sequence.rowTimes.length - 1] ?? 0) : 0;
    }
    return this.chipDuration;
  }

  play(offset: number): void {
    this.ensureStarted();
    const ctx = this.ctx!;
    if (this.mode === "chip" && !this.stemsReady()) return;
    if (this.mode === "sampler" && !this.samplerReady()) return;
    const wasSuspended = ctx.state !== "running";
    if (!this.songStarted) {
      const startTime = ctx.currentTime + 0.05;
      this.songStartOffset = offset;
      this.songStartCtxTime = startTime;
      this.songStarted = true;
      if (this.mode === "chip") {
        this.createStemSources(offset, startTime);
      } else if (this.sequence) {
        this.sampler.scheduler = new Scheduler(this.sequence, startTime, offset);
        this.startTimer(ctx);
      }
    }
    if (wasSuspended) void ctx.resume();
  }

  private startTimer(ctx: AudioContext): void {
    if (this.timer !== null) clearInterval(this.timer);
    const tick = () => this.sampler.tick(ctx, this.channelGain);
    tick();
    this.timer = setInterval(tick, 25);
  }

  private createStemSources(offset: number, startCtxTime: number): void {
    const ctx = this.ctx!;
    for (let c = 0; c < NUM_CHANNELS; c++) {
      const buffer = this.stemBuffers[c];
      const destination = this.channelGain[c];
      if (!buffer || !destination) continue;
      const source = ctx.createBufferSource();
      source.buffer = buffer;
      source.loop = true;
      source.connect(destination);
      source.start(startCtxTime, offset);
      this.stemSources[c] = source;
    }
  }

  private stopSources(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.sampler.clear();
    for (let c = 0; c < NUM_CHANNELS; c++) {
      const source = this.stemSources[c];
      if (!source) continue;
      try {
        source.stop();
        source.disconnect();
      } catch {
        /* ignore */
      }
      this.stemSources[c] = null;
    }
  }

  pause(): void {
    this.songStartOffset = this.currentTime();
    this.stopSources();
    this.songStarted = false;
  }

  stop(): void {
    this.stopPatternPreview();
    this.stopSources();
    this.songStarted = false;
    this.songStartOffset = 0;
  }

  seek(offset: number): void {
    const wasPlaying = this.isPlaying();
    this.stopSources();
    this.songStarted = false;
    this.songStartOffset = Math.max(offset, 0);
    if (wasPlaying) this.play(this.songStartOffset);
  }

  isPlaying(): boolean {
    return this.songStarted && this.ctx?.state === "running";
  }

  currentTime(): number {
    if (!this.songStarted || !this.ctx) return this.songStartOffset;
    const raw =
      this.songStartOffset + Math.max(this.ctx.currentTime - this.songStartCtxTime, 0);
    const duration = this.songDuration();
    if (duration > 0) return ((raw % duration) + duration) % duration;
    return raw;
  }

  setChannelVolume(channel: number, volume: number): void {
    this.channelVolume[channel] = volume;
    if (this.channelMuted[channel]) return;
    const gain = this.channelGain[channel];
    if (gain && this.ctx) gain.gain.setValueAtTime(volume, this.ctx.currentTime);
  }

  setChannelMute(channel: number, muted: boolean): void {
    this.channelMuted[channel] = muted;
    const gain = this.channelGain[channel];
    if (gain && this.ctx) {
      gain.gain.setValueAtTime(muted ? 0 : this.channelVolume[channel]!, this.ctx.currentTime);
    }
  }

  setMasterVolume(volume: number): void {
    this.masterVolume = volume;
    if (this.masterGain && this.ctx) {
      this.masterGain.gain.setValueAtTime(volume, this.ctx.currentTime);
    }
  }

  meterLevels(): number[] {
    const out: number[] = [0, 0, 0, 0, 0];
    const read = (analyser: AnalyserNode | null, index: number) => {
      if (!analyser) return;
      if (this.meterBuffer.length !== analyser.fftSize) {
        this.meterBuffer = new Float32Array(analyser.fftSize);
      }
      analyser.getFloatTimeDomainData(this.meterBuffer);
      let peak = 0;
      for (let i = 0; i < this.meterBuffer.length; i++) {
        const v = Math.abs(this.meterBuffer[i]!);
        if (v > peak) peak = v;
      }
      out[index] = peak;
    };
    for (let c = 0; c < NUM_CHANNELS; c++) read(this.channelAnalyser[c]!, c);
    read(this.masterAnalyser, NUM_CHANNELS);
    return out;
  }

  dispose(): void {
    this.stopPreview();
    this.stopPatternPreview();
    this.stopSamplePreview();
    this.stopSources();
    void this.ctx?.close();
    this.ctx = null;
  }
}
