import type { AudioClip } from "@/core/dsp";
import type { SpectralSettings } from "@/core/spectral";

export type WorkerRequest =
  | { id: number; kind: "ping" }
  | { id: number; kind: "render"; a: AudioClip; b: AudioClip | null; settings: SpectralSettings };

export type WorkerResponse =
  | { id: number; kind: "pong" }
  | { id: number; kind: "result"; result: AudioClip }
  | { id: number; kind: "error"; error: string };
