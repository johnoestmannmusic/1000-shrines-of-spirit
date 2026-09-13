import type { RawFurModule } from "@/core/fur/types";

export interface LoadedSong {
  /** Parsed `.fur`, when the song ships/uses one (absent for project-only songs). */
  raw?: RawFurModule;
  /** Original `.fur` bytes, retained for "Save .FUR" (absent for project-only songs). */
  furBytes?: Uint8Array;
  /** `lmp-default-proj.lampjson` contents as text. */
  project: string;
  stems: Array<Uint8Array | null>;
  samples: Array<Uint8Array | null>;
  chipMix: Uint8Array | null;
  error?: string;
}

export const IPC = {
  loadDefaultSong: "assets:load-default-song",
  chooseAudioFile: "assets:choose-audio-file",
  saveFile: "file:save",
  openExternal: "shell:open-external",
} as const;

export interface AudioFileChoice {
  name: string;
  bytes: Uint8Array;
}

export interface SaveFileRequest {
  suggestedName: string;
  bytes: Uint8Array;
}

export interface LanternApi {
  loadDefaultSong(): Promise<LoadedSong | { error: string }>;
  chooseAudioFile(): Promise<AudioFileChoice | { error: string }>;
  saveFile(suggestedName: string, bytes: Uint8Array): Promise<boolean>;
  openExternal(url: string): Promise<void>;
}
