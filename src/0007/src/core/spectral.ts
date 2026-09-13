import { audioClip, clipDuration, clipIsEmpty, type AudioClip } from "./dsp";

export const MIN_LOOP_SECONDS = 0.5;
export const MAX_LOOP_SECONDS = 8.0;
export const DEFAULT_LOOP_SECONDS = 4.0;

/** Stable kebab-case keys used in Project JSON. */
export type SpectralFusionMode =
  | "off"
  | "mix"
  | "cross-synth"
  | "convolve"
  | "ring-modulate"
  | "spectral-max"
  | "spectral-min"
  | "cycle";

export const SPECTRAL_FUSION_MODES: SpectralFusionMode[] = [
  "off",
  "mix",
  "cross-synth",
  "convolve",
  "ring-modulate",
  "spectral-max",
  "spectral-min",
  "cycle",
];

export function spectralModeLabel(mode: SpectralFusionMode): string {
  switch (mode) {
    case "off":
      return "Off (Freeze A only)";
    case "mix":
      return "Mix";
    case "cross-synth":
      return "Cross-Synth";
    case "convolve":
      return "Convolve";
    case "ring-modulate":
      return "Ring Modulate";
    case "spectral-max":
      return "Spectral Max";
    case "spectral-min":
      return "Spectral Min";
    case "cycle":
      return "Cycle";
  }
}

export function spectralModeNeedsB(mode: SpectralFusionMode): boolean {
  return mode !== "off";
}

export function spectralModeHasAmount(mode: SpectralFusionMode): boolean {
  return mode === "mix" || mode === "cross-synth" || mode === "convolve" || mode === "ring-modulate";
}

export interface SpectralSettings {
  enabled: boolean;
  sourceIndex2: number | null;
  mode: SpectralFusionMode;
  freezePoint: number;
  freezePointB: number;
  tune: number;
  tuneB: number;
  formantShift: number;
  formantShiftB: number;
  volume: number;
  volumeB: number;
  mixAmount: number;
  crossSynthAmount: number;
  convolveAmount: number;
  ringModAmount: number;
  stereoWidth: number;
  loopLengthSeconds: number;
  savedStartSec: number;
  savedEndSec: number;
  /** Sampler's loop flag, restored when Spectral is switched off. */
  savedLooping: boolean;
}

export function defaultSpectralSettings(): SpectralSettings {
  return {
    enabled: false,
    sourceIndex2: null,
    mode: "off",
    freezePoint: 50,
    freezePointB: 50,
    tune: 0,
    tuneB: 0,
    formantShift: 0,
    formantShiftB: 0,
    volume: 100,
    volumeB: 100,
    mixAmount: 50,
    crossSynthAmount: 100,
    convolveAmount: 100,
    ringModAmount: 100,
    stereoWidth: 30,
    loopLengthSeconds: DEFAULT_LOOP_SECONDS,
    savedStartSec: 0,
    savedEndSec: 0,
    savedLooping: false,
  };
}

/** True once the prism_dsp WASM module has been initialised. */
let wasmAvailable = false;
export function setSpectralWasmAvailable(available: boolean): void {
  wasmAvailable = available;
}
export function spectralWasmAvailable(): boolean {
  return wasmAvailable;
}

export interface SpectralRenderFn {
  (a: AudioClip, b: AudioClip | null, settings: SpectralSettings): Promise<AudioClip>;
}

let renderImpl: SpectralRenderFn | null = null;
export function registerSpectralRenderer(fn: SpectralRenderFn | null): void {
  renderImpl = fn;
}

/**
 * Renders Sample A (required) optionally fused with Sample B through the
 * prism_dsp WASM module (on the Spectral Worker when available, else the
 * main thread), normalised so the loudest sample hits full scale.
 */
export async function spectralRender(
  a: AudioClip,
  b: AudioClip | null,
  settings: SpectralSettings,
): Promise<AudioClip> {
  if (clipIsEmpty(a)) throw new Error("Sample A is empty");
  if (spectralModeNeedsB(settings.mode) && (!b || clipIsEmpty(b))) {
    throw new Error("Sample B is required");
  }
  if (!renderImpl) {
    throw new Error("Spectral engine (prism_dsp WASM) is not available yet");
  }
  const clip = await renderImpl(a, b, settings);
  normalizePeak(clip.channels);
  return clip;
}

export { clipDuration };

function normalizePeak(channels: Float32Array[]): void {
  let peak = 0;
  for (const channel of channels) {
    for (let i = 0; i < channel.length; i++) {
      const v = Math.abs(channel[i]!);
      if (v > peak) peak = v;
    }
  }
  if (peak <= 1e-9 || !Number.isFinite(peak)) return;
  const gain = 1 / peak;
  for (const channel of channels) {
    for (let i = 0; i < channel.length; i++) channel[i] = channel[i]! * gain;
  }
}

/** Convenience used by tests to build a clip from plain arrays. */
export function makeClip(channels: number[][], sampleRate: number): AudioClip {
  return audioClip(
    channels.map((c) => Float32Array.from(c)),
    sampleRate,
  );
}
