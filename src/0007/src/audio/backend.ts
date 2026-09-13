import type { AudioClip } from "@/core/dsp";
import type { MasterFxSettings } from "@/core/masterFx";
import type { SamplerSettings, Sequence } from "@/core/sampler";

export type PlaybackMode = "chip" | "sampler";

export const NUM_CHANNELS = 4;

export interface SamplePlayhead {
  source: number;
  instrument: number | null;
  position: number;
  fused: boolean;
  level: number;
}

export interface PatternNote {
  channel: number;
  instrument: number;
  /** Pitch ratio before the instrument's transpose is applied. */
  rate: number;
  volume: number;
}

export interface AudioBackend {
  loadSampler(
    sequence: Sequence,
    settings: SamplerSettings[],
    samples: Array<Uint8Array | null>,
  ): void;
  updateSequence(sequence: Sequence): void;
  replaceSample(source: number, bytes: Uint8Array | null): void;
  previewSample(source: number): void;
  stopSamplePreview(): void;
  samplePlayheads(): SamplePlayhead[];
  sampleWaveform(source: number): Array<[number, number]>;
  sampleClip(source: number): AudioClip | null;
  effectiveClip(instrument: number): AudioClip | null;
  effectiveWaveform(instrument: number): Array<[number, number]>;
  fusionWaveform(instrument: number): Array<[number, number]>;
  effectiveDuration(instrument: number): number;
  fusionReady(instrument: number): boolean;
  fusionRendering(instrument: number): boolean;
  renderFusion(instrument: number): boolean;
  takeFusionCompleted(instrument: number): boolean;
  preview(instrument: number, reference: boolean): void;
  stopPreview(): void;
  previewPosition(): { instrument: number; position: number } | null;
  previewPattern(
    channels: number[],
    offset: number,
    duration: number,
    notes: PatternNote[],
  ): void;
  stopPatternPreview(): void;
  samplerReady(): boolean;
  sampleDurations(): number[];
  setSamplerSettings(instrument: number, settings: SamplerSettings): void;
  setMode(mode: PlaybackMode): void;
  error(): string | null;
  ensureStarted(): void;
  loadStems(bytes: Array<Uint8Array | null>): void;
  stemsReady(): boolean;
  songDuration(): number;
  play(offset: number): void;
  pause(): void;
  stop(): void;
  seek(offset: number): void;
  isPlaying(): boolean;
  currentTime(): number;
  setChannelVolume(channel: number, volume: number): void;
  setChannelMute(channel: number, muted: boolean): void;
  setMasterVolume(volume: number): void;
  setMasterFx(settings: MasterFxSettings): void;
  meterLevels(): number[];
  dispose(): void;
}
