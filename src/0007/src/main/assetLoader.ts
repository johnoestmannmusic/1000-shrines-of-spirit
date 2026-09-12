import { app, dialog } from "electron";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseFurFile } from "../core/fur/node";
import { assembleSongFolder } from "./folder";
import type { LoadedSong, SongFolderFile } from "../shared/types";

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
  const furBytes = readIfPresent(path.join(dir, "flight_school_night_shift.fur"));
  if (!furBytes) {
    return { error: `Cannot find bundled song assets in ${dir}` };
  }
  const project = readTextIfPresent(path.join(dir, "lmp-default-proj.json")) ?? "";
  const stems = [0, 1, 2, 3].map((i) => readIfPresent(path.join(dir, `${i}.ogg`)));
  const samples = [0, 1, 2].map((i) =>
    readIfPresent(path.join(dir, "SourceSamples", `${i}.ogg`)),
  );
  const chipMix = readIfPresent(path.join(dir, "flight_school_night_shift.wav"));
  try {
    const raw = parseFurFile(furBytes);
    return { raw, furBytes, project, stems, samples, chipMix };
  } catch (e) {
    return { error: `Cannot parse bundled .fur: ${String(e)}` };
  }
}

function collectFiles(root: string, current: string, out: SongFolderFile[]): void {
  for (const entry of readdirSync(current)) {
    const full = path.join(current, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      collectFiles(root, full, out);
    } else {
      const rel = path.relative(root, full).split(path.sep).join("/");
      out.push({ name: rel, bytes: new Uint8Array(readFileSync(full)) });
    }
  }
}

export async function loadSongFolder(): Promise<LoadedSong | { error: string }> {
  const result = await dialog.showOpenDialog({ properties: ["openDirectory"] });
  if (result.canceled || result.filePaths.length === 0) return { error: "cancelled" };
  try {
    const out: SongFolderFile[] = [];
    collectFiles(result.filePaths[0]!, result.filePaths[0]!, out);
    return assembleSongFolder(out);
  } catch (e) {
    return { error: `Cannot read folder: ${String(e)}` };
  }
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

export async function saveFile(suggestedName: string, bytes: Uint8Array): Promise<boolean> {
  const result = await dialog.showSaveDialog({ defaultPath: suggestedName });
  if (result.canceled || !result.filePath) return false;
  writeFileSync(result.filePath, bytes);
  return true;
}
