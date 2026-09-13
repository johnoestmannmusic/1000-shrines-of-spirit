import { app, dialog } from "electron";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseFurFile } from "../core/fur/node";
import type { LoadedSong } from "../shared/types";

function assetsDir(): string {
  return app.isPackaged
    ? path.join(process.resourcesPath, "assets")
    : path.join(app.getAppPath(), "assets");
}

function readIfPresent(filePath: string): Uint8Array | null {
  if (!existsSync(filePath)) return null;
  return new Uint8Array(readFileSync(filePath));
}

function readTextIfPresent(filePath: string): string | null {
  if (!existsSync(filePath)) return null;
  return readFileSync(filePath, "utf8");
}

export function loadDefaultSong(): LoadedSong | { error: string } {
  const dir = assetsDir();
  const project = readTextIfPresent(path.join(dir, "lmp-default-proj.lampjson"));
  if (!project) {
    return { error: `Cannot find the bundled project assets in ${dir}` };
  }
  const stems = [0, 1, 2, 3].map((i) => readIfPresent(path.join(dir, `${i}.ogg`)));
  const samples = [0, 1, 2, 3, 4, 5].map((i) =>
    readIfPresent(path.join(dir, "SourceSamples", `${i}.ogg`)),
  );
  const chipMix = readIfPresent(path.join(dir, "flight_school_night_shift.wav"));
  // The .fur (and its stems) are optional: a project-only song loads without
  // CHIP MODE, reconstructing its model from the project's pattern snapshot.
  const furBytes = readIfPresent(path.join(dir, "flight_school_night_shift.fur"));
  let raw;
  if (furBytes) {
    try {
      raw = parseFurFile(furBytes);
    } catch (e) {
      return { error: `Cannot parse bundled .fur: ${String(e)}` };
    }
  }
  return { raw, furBytes: furBytes ?? undefined, project, stems, samples, chipMix };
}

export async function chooseAudioFile(): Promise<
  { name: string; bytes: Uint8Array } | { error: string }
> {
  const result = await dialog.showOpenDialog({
    properties: ["openFile"],
    filters: [{ name: "Audio", extensions: ["wav", "ogg", "mp3"] }],
  });
  if (result.canceled || result.filePaths.length === 0) return { error: "cancelled" };
  const filePath = result.filePaths[0]!;
  try {
    const bytes = new Uint8Array(readFileSync(filePath));
    const base = path.basename(filePath).replace(/\.[^.]+$/, "");
    return { name: base, bytes };
  } catch (e) {
    return { error: `Cannot read ${filePath}: ${String(e)}` };
  }
}

const SAVE_FILTERS: Record<string, { name: string; extensions: string[] }> = {
  lampjson: { name: "Lantern Project", extensions: ["lampjson"] },
  wav: { name: "WAV audio", extensions: ["wav"] },
  mid: { name: "MIDI", extensions: ["mid"] },
  zip: { name: "ZIP archive", extensions: ["zip"] },
  png: { name: "PNG image", extensions: ["png"] },
  fur: { name: "Furnace module", extensions: ["fur"] },
};

export async function saveFile(suggestedName: string, bytes: Uint8Array): Promise<boolean> {
  const extension = path.extname(suggestedName).replace(".", "").toLowerCase();
  const filter = SAVE_FILTERS[extension];
  const result = await dialog.showSaveDialog({
    defaultPath: suggestedName,
    filters: filter ? [filter] : undefined,
  });
  if (result.canceled || !result.filePath) return false;
  writeFileSync(result.filePath, bytes);
  return true;
}
