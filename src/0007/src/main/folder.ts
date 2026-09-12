import { parseFurFile } from "../core/fur/node";
import { defaultProject, projectToValue, type ProjectFile } from "../core/project";
import { defaultSamplerSettings } from "../core/sampler";
import type { LoadedSong, SongFolderFile } from "../shared/types";

function norm(p: string): string {
  return "/" + p.replace(/\\/g, "/").replace(/^\/+/, "").toLowerCase();
}

function isAudio(name: string): boolean {
  return /\.(ogg|wav|mp3)$/i.test(name);
}

/**
 * Port of lantern-app's `assemble_song_folder`: requires exactly one `.fur`,
 * optional 0- or 1-based numbered stems, optional source samples under
 * `assets/SourceSamples`, and an optional `lmp-default-proj.json` (else a
 * default project is synthesized).
 */
export function assembleSongFolder(files: SongFolderFile[]): LoadedSong | { error: string } {
  const furs = files.filter((f) => f.name.toLowerCase().endsWith(".fur"));
  if (furs.length !== 1) {
    return {
      error: `Choose a folder containing exactly one .fur file and its ASSETS folder (found ${furs.length}).`,
    };
  }
  const furFile = furs[0]!;
  let raw;
  try {
    raw = parseFurFile(furFile.bytes);
  } catch (e) {
    return { error: `Selected .fur is unsupported: ${String(e)}` };
  }

  const findIn = (folder: string, slot: number): Uint8Array | null => {
    const match = files.find(
      (f) => isAudio(f.name) && norm(f.name).includes(`/${folder}/${slot}.`),
    );
    return match?.bytes ?? null;
  };

  const findStem = (slot: number): Uint8Array | null => {
    const match = files.find(
      (f) =>
        isAudio(f.name) &&
        !norm(f.name).includes("/sourcesamples/") &&
        new RegExp(`/${slot}\\.(ogg|wav|mp3)$`).test(norm(f.name)),
    );
    return match?.bytes ?? null;
  };

  const firstStem = findStem(0) ? 0 : 1;
  const stems = [0, 1, 2, 3].map((i) => findStem(firstStem + i));
  const stemsOptional = stems.some((s) => !s);
  const samples = [0, 1, 2].map((i) => findIn("assets/sourcesamples", i));

  let projectText: string;
  const projectFile = files.find((f) =>
    norm(f.name).endsWith("/assets/lmp-default-proj.json"),
  );
  if (projectFile) {
    projectText = new TextDecoder().decode(projectFile.bytes);
  } else {
    const project: ProjectFile = defaultProject();
    project.samplerModeEnabled = stemsOptional;
    project.songTitle = raw.info.name;
    project.artist = raw.info.author;
    project.comments = raw.subsongs[0]?.comment ?? "";
    project.instruments = raw.instruments.map(() => defaultSamplerSettings());
    project.mutedInstruments = raw.instruments.map(() => false);
    project.sourceSamples = Array.from({ length: 6 }, (_, slot) =>
      slot < 3 && samples[slot]
        ? {
            name: `Source ${slot}`,
            url: `ASSETS/SourceSamples/${slot}.ogg`,
            comments: "",
            dataUrl: null,
          }
        : null,
    );
    projectText = JSON.stringify(projectToValue(project));
  }

  const chipMix =
    files.find((f) => {
      if (!isAudio(f.name) || !f.name.toLowerCase().endsWith(".wav")) return false;
      if (norm(f.name).includes("/sourcesamples/")) return false;
      const base = (f.name.split("/").pop() ?? "").replace(/\.wav$/i, "");
      return !/^[0-9]+$/.test(base);
    })?.bytes ?? null;

  return {
    raw,
    furBytes: furFile.bytes,
    project: projectText,
    stems,
    samples,
    chipMix,
  };
}
