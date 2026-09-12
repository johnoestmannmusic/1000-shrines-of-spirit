import { parse, startsWithMagic } from "@/core/fur/parse";
import type { AudioFileChoice, LoadedSong, SongFolderFile } from "../shared/types";

/** True when running inside the Electron shell (preload bridge present). */
export const isDesktop =
  typeof window !== "undefined" && typeof (window as { lantern?: unknown }).lantern !== "undefined";

function assetUrl(path: string): string {
  return `${import.meta.env.BASE_URL}assets/${path}`;
}

async function fetchBytes(path: string): Promise<Uint8Array | null> {
  try {
    const response = await fetch(assetUrl(path));
    if (!response.ok) return null;
    return new Uint8Array(await response.arrayBuffer());
  } catch {
    return null;
  }
}

/** zlib-inflate a `.fur` file in the browser (Furnace wraps the whole file). */
async function inflateFur(bytes: Uint8Array): Promise<Uint8Array> {
  if (startsWithMagic(bytes)) return bytes;
  if (typeof DecompressionStream === "undefined") {
    throw new Error("This browser cannot decompress Furnace modules (no DecompressionStream)");
  }
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(new DecompressionStream("deflate"));
  const inflated = await new Response(stream).arrayBuffer();
  return new Uint8Array(inflated);
}

/** Loads the bundled song either from Electron main or from the served assets. */
export async function loadDefaultSong(): Promise<LoadedSong | { error: string }> {
  if (isDesktop) return window.lantern.loadDefaultSong();

  const furBytes = await fetchBytes("flight_school_night_shift.fur");
  if (!furBytes) {
    return { error: "Missing assets/flight_school_night_shift.fur" };
  }
  const projectResponse = await fetch(assetUrl("lmp-default-proj.json")).catch(() => null);
  const project = projectResponse && projectResponse.ok ? await projectResponse.text() : "";
  const stems = await Promise.all([0, 1, 2, 3].map((i) => fetchBytes(`${i}.ogg`)));
  const samples = await Promise.all([0, 1, 2].map((i) => fetchBytes(`SourceSamples/${i}.ogg`)));

  try {
    const raw = parse(await inflateFur(furBytes));
    return { raw, furBytes, project, stems, samples, chipMix: null };
  } catch (e) {
    return { error: `Cannot parse bundled .fur: ${String(e)}` };
  }
}

/** Saves bytes: native dialog on desktop, Blob download on the web. */
export async function saveFile(name: string, bytes: Uint8Array): Promise<boolean> {
  if (isDesktop) return window.lantern.saveFile(name, bytes);
  const blob = new Blob([bytes as BlobPart]);
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  // Keep the object URL alive briefly so the download can start.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}

/** Picks one audio file (Electron dialog or a hidden file input). */
export function chooseAudioFile(): Promise<AudioFileChoice | { error: string }> {
  if (isDesktop) return window.lantern.chooseAudioFile();
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "audio/wav,audio/ogg,audio/mpeg,.wav,.ogg,.mp3";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) {
        resolve({ error: "cancelled" });
        return;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      resolve({ name: file.name.replace(/\.[^.]+$/, ""), bytes });
    };
    input.click();
  });
}

/** Folder loading is an Electron-only convenience; unsupported on the web. */
export function loadSongFolder(): Promise<LoadedSong | { error: string }> {
  if (isDesktop) return window.lantern.loadSongFolder();
  return Promise.resolve({ error: "Folder loading is not supported in the browser build" });
}

export type { LoadedSong, AudioFileChoice, SongFolderFile };
