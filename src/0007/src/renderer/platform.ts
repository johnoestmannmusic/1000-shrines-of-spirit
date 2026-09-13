import type { AudioFileChoice, LoadedSong } from "../shared/types";

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

/** Loads the bundled song either from Electron main or from the served assets. */
export async function loadDefaultSong(): Promise<LoadedSong | { error: string }> {
  if (isDesktop) return window.lantern.loadDefaultSong();

  const projectResponse = await fetch(assetUrl("lmp-default-proj.lampjson")).catch(() => null);
  const project = projectResponse && projectResponse.ok ? await projectResponse.text() : "";
  if (!project) {
    return { error: "Missing assets/lmp-default-proj.lampjson" };
  }
  const samples = await Promise.all(
    [0, 1, 2, 3, 4, 5].map((i) => fetchBytes(`SourceSamples/${i}.ogg`)),
  );

  // The bundled song is project-only (no Furnace .fur / CHIP stems), so no
  // CHIP assets are requested — a missing file would otherwise log a 404.
  return { project, stems: [null, null, null, null], samples, chipMix: null };
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

/** Opens a URL in the system browser (desktop) or a new tab (web). */
export async function openExternal(url: string): Promise<void> {
  if (isDesktop) return window.lantern.openExternal(url);
  window.open(url, "_blank", "noopener,noreferrer");
}

export type { LoadedSong, AudioFileChoice };

