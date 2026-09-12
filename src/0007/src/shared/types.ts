import type { RawFurModule } from "@/core/fur/types";

export interface LoadedSong {
  raw: RawFurModule;
  /** Original `.fur` bytes, retained for "Save .FUR". */
  furBytes: Uint8Array;
  /** `lmp-default-proj.json` contents as text. */
  project: string;
  stems: Array<Uint8Array | null>;
  samples: Array<Uint8Array | null>;
  chipMix: Uint8Array | null;
  error?: string;
}

export interface SongFolderFile {
  name: string;
  bytes: Uint8Array;
}

export const IPC = {
  loadDefaultSong: "assets:load-default-song",
  loadSongFolder: "assets:load-song-folder",
  chooseAudioFile: "assets:choose-audio-file",
  saveFile: "file:save",
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
  loadSongFolder(): Promise<LoadedSong | { error: string }>;
  chooseAudioFile(): Promise<AudioFileChoice | { error: string }>;
  saveFile(suggestedName: string, bytes: Uint8Array): Promise<boolean>;
}
