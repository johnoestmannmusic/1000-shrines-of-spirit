import { A_REF_NOTE } from "./pitch";
import {
  defaultSamplerSettings,
  type SamplerSettings,
} from "./sampler";
import type { NoteValue, PatternCell } from "./fur/types";
import { SPECTRAL_FUSION_MODES, defaultSpectralSettings, type SpectralSettings } from "./spectral";
import { retime, type PatternSnapshot, type SongModel } from "./songModel";

export interface SourceSampleRef {
  name: string;
  url: string | null;
  comments: string;
  dataUrl?: string | null;
}

export interface ProjectFile {
  version: number;
  samplerModeEnabled: boolean;
  channelVolume: number[];
  masterVolume: number;
  mutedChannels: boolean[];
  mutedInstruments: boolean[];
  refPitchEnabled: boolean;
  sourceSamples: Array<SourceSampleRef | null>;
  instruments: SamplerSettings[];
  songTitle: string;
  artist: string;
  album: string;
  comments: string;
  musicLicense: string;
  codeLicense: string;
  viewSourceLink: string;
  websiteLink: string;
  theme: string;
  patternSnapshot?: PatternSnapshot | null;
  tickRateOverride?: number | null;
  speedOverride?: number | null;
  highlightAOverride?: number | null;
  highlightBOverride?: number | null;
  virtualTempoOverride?: [number, number] | null;
}

export function defaultProject(): ProjectFile {
  return {
    version: 1,
    samplerModeEnabled: true,
    channelVolume: [1, 1, 1, 1],
    masterVolume: 1,
    mutedChannels: [false, false, false, false],
    mutedInstruments: [],
    refPitchEnabled: false,
    sourceSamples: Array.from({ length: 6 }, () => null),
    instruments: [],
    songTitle: "",
    artist: "",
    album: "",
    comments: "",
    musicLicense: "",
    codeLicense: "",
    viewSourceLink: "",
    websiteLink: "",
    theme: "system",
    patternSnapshot: null,
    tickRateOverride: null,
    speedOverride: null,
    highlightAOverride: null,
    highlightBOverride: null,
    virtualTempoOverride: null,
  };
}

// ---- NoteValue serde compatibility (Rust derived serde representation) ----

export function noteToSerde(note: NoteValue): unknown {
  switch (note.kind) {
    case "note":
      return { Note: note.note };
    case "off":
      return "Off";
    case "release":
      return "Release";
    case "macroRelease":
      return "MacroRelease";
    case "rawFreq":
      return { RawFreq: note.value };
  }
}

export function noteFromSerde(value: unknown): NoteValue | null {
  if (value === "Off") return { kind: "off" };
  if (value === "Release") return { kind: "release" };
  if (value === "MacroRelease") return { kind: "macroRelease" };
  if (value && typeof value === "object") {
    const obj = value as Record<string, unknown>;
    if (typeof obj.Note === "number") return { kind: "note", note: obj.Note };
    if (typeof obj.RawFreq === "number") return { kind: "rawFreq", value: obj.RawFreq };
  }
  return null;
}

function cellToSerde(cell: PatternCell): unknown {
  return {
    note: cell.note ? noteToSerde(cell.note) : null,
    instrument: cell.instrument,
    volume: cell.volume,
    effects: cell.effects.map((e) => ({ effect: e.effect, value: e.value })),
  };
}

function cellFromSerde(value: unknown): PatternCell {
  const obj = (value ?? {}) as Record<string, unknown>;
  const effectsRaw = Array.isArray(obj.effects) ? obj.effects : [];
  const effects = effectsRaw.map((e) => {
    const slot = (e ?? {}) as Record<string, unknown>;
    return {
      effect: typeof slot.effect === "number" ? slot.effect : null,
      value: typeof slot.value === "number" ? slot.value : null,
    };
  });
  while (effects.length < 8) effects.push({ effect: null, value: null });
  return {
    note: noteFromSerde(obj.note),
    instrument: typeof obj.instrument === "number" ? obj.instrument : null,
    volume: typeof obj.volume === "number" ? obj.volume : null,
    effects,
  };
}

function cellIsEmpty(cell: PatternCell): boolean {
  return (
    cell.note === null &&
    cell.instrument === null &&
    cell.volume === null &&
    cell.effects.every((e) => e.effect === null && e.value === null)
  );
}

export function snapshotToSerde(snapshot: PatternSnapshot | null | undefined): unknown {
  if (!snapshot) return null;
  return {
    orderLength: snapshot.orderLength,
    channels: snapshot.channels.map((channel) => ({
      orderList: channel.orderList,
      // Sparse rows: only cells that actually contain something are written,
      // as [rowIndex, cell] pairs. Empty patterns collapse to `[]`.
      patterns: channel.patterns.map(([index, rows]) => {
        const sparse: unknown[] = [];
        rows.forEach((cell, row) => {
          if (!cellIsEmpty(cell)) sparse.push([row, cellToSerde(cell)]);
        });
        return [index, sparse];
      }),
    })),
  };
}

export function snapshotFromSerde(value: unknown): PatternSnapshot | null {
  if (!value || typeof value !== "object") return null;
  const obj = value as Record<string, unknown>;
  const channelsRaw = Array.isArray(obj.channels) ? obj.channels : [];
  return {
    orderLength: typeof obj.orderLength === "number" ? obj.orderLength : 0,
    channels: channelsRaw.map((channelRaw) => {
      const channel = (channelRaw ?? {}) as Record<string, unknown>;
      const patternsRaw = Array.isArray(channel.patterns) ? channel.patterns : [];
      return {
        orderList: Array.isArray(channel.orderList) ? (channel.orderList as number[]) : [],
        patterns: patternsRaw.map((pair) => {
          const [index, rows] = pair as [number, unknown[]];
          const list = rows ?? [];
          const first = list[0];
          const sparse = Array.isArray(first) && typeof (first as unknown[])[0] === "number";
          if (sparse) {
            const cells: PatternCell[] = [];
            for (const entry of list as Array<[number, unknown]>) {
              cells[entry[0]] = cellFromSerde(entry[1]);
            }
            for (let i = 0; i < cells.length; i++) {
              if (!cells[i]) cells[i] = cellFromSerde(null);
            }
            return [index, cells] as [number, PatternCell[]];
          }
          return [index, list.map(cellFromSerde)] as [number, PatternCell[]];
        }),
      };
    }),
  };
}

// ---- Sampler / spectral settings JSON mapping ----

/** Legacy pre-prism_dsp Fusion mode keys that map onto a current mode. */
const LEGACY_MODE_ALIASES: Record<string, SpectralSettings["mode"]> = {
  "spectral-blend": "cross-synth",
};

function spectralFromJson(value: unknown): SpectralSettings {
  const d = defaultSpectralSettings();
  if (!value || typeof value !== "object") return d;
  const obj = value as Record<string, unknown>;
  const num = (key: string, fallback: number) =>
    typeof obj[key] === "number" ? (obj[key] as number) : fallback;
  const rawMode =
    typeof obj.mode === "string"
      ? obj.mode
      : typeof obj.algorithm === "string"
        ? obj.algorithm
        : d.mode;
  const aliased = LEGACY_MODE_ALIASES[rawMode] ?? rawMode;
  const mode = (SPECTRAL_FUSION_MODES as string[]).includes(aliased)
    ? (aliased as SpectralSettings["mode"])
    : "off";
  return {
    enabled: typeof obj.enabled === "boolean" ? obj.enabled : d.enabled,
    sourceIndex2: typeof obj.sourceIndex2 === "number" ? obj.sourceIndex2 : null,
    mode,
    freezePoint: num("freezePoint", d.freezePoint),
    freezePointB: num("freezePointB", d.freezePointB),
    tune: num("tune", d.tune),
    tuneB: num("tuneB", d.tuneB),
    formantShift: num("formantShift", d.formantShift),
    formantShiftB: num("formantShiftB", d.formantShiftB),
    volume: num("volume", d.volume),
    volumeB: num("volumeB", d.volumeB),
    mixAmount: num("mixAmount", d.mixAmount),
    crossSynthAmount: num("crossSynthAmount", d.crossSynthAmount),
    convolveAmount: num("convolveAmount", d.convolveAmount),
    ringModAmount: num("ringModAmount", d.ringModAmount),
    stereoWidth: num("stereoWidth", d.stereoWidth),
    loopLengthSeconds: num("loopLengthSeconds", d.loopLengthSeconds),
    savedStartSec: num("savedStartSec", d.savedStartSec),
    savedEndSec: num("savedEndSec", d.savedEndSec),
  };
}

export function samplerFromJson(value: unknown): SamplerSettings {
  const d = defaultSamplerSettings();
  if (!value || typeof value !== "object") return d;
  const obj = value as Record<string, unknown>;
  const num = (key: string, fallback: number) =>
    typeof obj[key] === "number" ? (obj[key] as number) : fallback;
  const bool = (key: string, fallback: boolean) =>
    typeof obj[key] === "boolean" ? (obj[key] as boolean) : fallback;
  return {
    sourceIndex: typeof obj.sourceIndex === "number" ? obj.sourceIndex : null,
    startSec: num("startSec", d.startSec),
    endSec: num("endSec", d.endSec),
    transpose: num("transpose", d.transpose),
    volume: num("volume", d.volume),
    looping: bool("loop", d.looping),
    pingPong: bool("pingPong", d.pingPong),
    attack: num("attack", d.attack),
    decay: num("decay", d.decay),
    sustain: num("sustain", d.sustain),
    release: num("release", d.release),
    pan: num("pan", d.pan),
    panRandomRange: num("panRandomRange", d.panRandomRange),
    polyphonic: bool("polyphonic", d.polyphonic),
    voiceCap: num("voiceCap", d.voiceCap),
    spectral: spectralFromJson(obj.spectral ?? obj.spectralFusion),
    muted: false,
  };
}

export function samplerToJson(settings: SamplerSettings, includeMuted = false): Record<string, unknown> {
  const out: Record<string, unknown> = {
    sourceIndex: settings.sourceIndex,
    startSec: settings.startSec,
    endSec: settings.endSec,
    transpose: settings.transpose,
    volume: settings.volume,
    loop: settings.looping,
    pingPong: settings.pingPong,
    attack: settings.attack,
    decay: settings.decay,
    sustain: settings.sustain,
    release: settings.release,
    pan: settings.pan,
    panRandomRange: settings.panRandomRange,
    polyphonic: settings.polyphonic,
    voiceCap: settings.voiceCap,
    spectral: settings.spectral,
  };
  if (includeMuted) out.muted = settings.muted;
  return out;
}

function sourceSampleFromJson(value: unknown): SourceSampleRef | null {
  if (!value || typeof value !== "object") return null;
  const obj = value as Record<string, unknown>;
  return {
    name: typeof obj.name === "string" ? obj.name : "",
    url: typeof obj.url === "string" ? obj.url : null,
    comments: typeof obj.comments === "string" ? obj.comments : "",
    dataUrl: typeof obj.dataUrl === "string" ? obj.dataUrl : null,
  };
}

/** Parses JSON, applying the legacy `rootNote` -> `transpose` migration. */
export function projectFromJson(text: string): ProjectFile {
  let value: Record<string, unknown>;
  try {
    value = JSON.parse(text) as Record<string, unknown>;
  } catch (e) {
    throw new Error(`Invalid Project JSON: ${String(e)}`);
  }
  return projectFromValue(value);
}

export function projectFromValue(value: Record<string, unknown>): ProjectFile {
  const base = defaultProject();

  const instrumentsRaw = Array.isArray(value.instruments) ? value.instruments : [];
  const instruments = instrumentsRaw.map((raw) => {
    if (raw && typeof raw === "object") {
      const obj = raw as Record<string, unknown>;
      if (obj.transpose === undefined && typeof obj.rootNote === "number") {
        obj.transpose = A_REF_NOTE - obj.rootNote;
      }
      delete obj.rootNote;
    }
    return samplerFromJson(raw);
  });

  const sourceSamplesRaw = Array.isArray(value.sourceSamples) ? value.sourceSamples : [];
  const sourceSamples: Array<SourceSampleRef | null> = Array.from({ length: 6 }, (_, i) =>
    sourceSampleFromJson(sourceSamplesRaw[i]),
  );

  const arr = <T>(v: unknown, fallback: T[]): T[] => (Array.isArray(v) ? (v as T[]) : fallback);
  const str = (key: string, fallback: string) =>
    typeof value[key] === "string" ? (value[key] as string) : fallback;
  const num = (key: string, fallback: number) =>
    typeof value[key] === "number" ? (value[key] as number) : fallback;

  let virtualTempo: [number, number] | null = null;
  if (Array.isArray(value.virtualTempoOverride) && value.virtualTempoOverride.length === 2) {
    const [a, b] = value.virtualTempoOverride as number[];
    virtualTempo = [a as number, b as number];
  }

  return {
    ...base,
    version: num("version", 1),
    samplerModeEnabled:
      typeof value.samplerModeEnabled === "boolean" ? value.samplerModeEnabled : true,
    channelVolume: arr<number>(value.channelVolume, [1, 1, 1, 1]).slice(0, 4),
    masterVolume: num("masterVolume", 1),
    mutedChannels: arr<boolean>(value.mutedChannels, [false, false, false, false]).slice(0, 4),
    mutedInstruments: arr<boolean>(value.mutedInstruments, []),
    refPitchEnabled: typeof value.refPitchEnabled === "boolean" ? value.refPitchEnabled : false,
    sourceSamples,
    instruments,
    songTitle: str("songTitle", ""),
    artist: str("artist", ""),
    album: str("album", ""),
    comments: str("comments", ""),
    musicLicense: str("musicLicense", ""),
    codeLicense: str("codeLicense", ""),
    viewSourceLink: str("viewSourceLink", ""),
    websiteLink: str("websiteLink", ""),
    theme: str("theme", "system"),
    patternSnapshot: snapshotFromSerde(value.patternSnapshot),
    tickRateOverride: typeof value.tickRateOverride === "number" ? value.tickRateOverride : null,
    speedOverride: typeof value.speedOverride === "number" ? value.speedOverride : null,
    highlightAOverride:
      typeof value.highlightAOverride === "number" ? value.highlightAOverride : null,
    highlightBOverride:
      typeof value.highlightBOverride === "number" ? value.highlightBOverride : null,
    virtualTempoOverride: virtualTempo,
  };
}

export function projectToJson(project: ProjectFile, pretty = false): string {
  const value = projectToValue(project);
  return pretty ? JSON.stringify(value, null, 2) : JSON.stringify(value);
}

export function projectToValue(project: ProjectFile): Record<string, unknown> {
  const value: Record<string, unknown> = {
    version: project.version,
    samplerModeEnabled: project.samplerModeEnabled,
    channelVolume: project.channelVolume,
    masterVolume: project.masterVolume,
    refPitchEnabled: project.refPitchEnabled,
    instruments: project.instruments.map((s) => samplerToJson(s, false)),
    theme: project.theme,
  };
  if (project.mutedChannels.some(Boolean)) value.mutedChannels = project.mutedChannels;
  if (project.mutedInstruments.some(Boolean)) value.mutedInstruments = project.mutedInstruments;
  if (project.sourceSamples.some((s) => s !== null)) value.sourceSamples = project.sourceSamples;
  if (project.songTitle) value.songTitle = project.songTitle;
  if (project.artist) value.artist = project.artist;
  if (project.album) value.album = project.album;
  if (project.comments) value.comments = project.comments;
  if (project.musicLicense) value.musicLicense = project.musicLicense;
  if (project.codeLicense) value.codeLicense = project.codeLicense;
  if (project.viewSourceLink) value.viewSourceLink = project.viewSourceLink;
  if (project.websiteLink) value.websiteLink = project.websiteLink;
  if (project.patternSnapshot) value.patternSnapshot = snapshotToSerde(project.patternSnapshot);
  if (project.tickRateOverride != null) value.tickRateOverride = project.tickRateOverride;
  if (project.speedOverride != null) value.speedOverride = project.speedOverride;
  if (project.highlightAOverride != null) value.highlightAOverride = project.highlightAOverride;
  if (project.highlightBOverride != null) value.highlightBOverride = project.highlightBOverride;
  if (project.virtualTempoOverride != null)
    value.virtualTempoOverride = project.virtualTempoOverride;
  return value;
}

export function validateProject(project: ProjectFile, instrumentCount: number): void {
  if (project.version !== 1) {
    throw new Error(`Unsupported Project JSON version ${project.version}`);
  }
  if (project.instruments.length === 0) {
    throw new Error("Project JSON has no instrument settings");
  }
  if (project.instruments.length > instrumentCount) {
    throw new Error(
      `Project has ${project.instruments.length} instruments but this song has ${instrumentCount}`,
    );
  }
  if (
    !Number.isFinite(project.masterVolume) ||
    project.channelVolume.some((v) => !Number.isFinite(v))
  ) {
    throw new Error("Project mixer values must be finite numbers");
  }
  if (!["system", "light", "dark"].includes(project.theme)) {
    throw new Error("Project theme must be system, light, or dark");
  }
}

export function applyTimingOverrides(project: ProjectFile, song: SongModel): void {
  let changed = false;
  if (project.tickRateOverride != null) {
    song.meta.tickRate = project.tickRateOverride;
    changed = true;
  }
  if (project.speedOverride != null) {
    if (song.meta.speedPattern.length > 0) song.meta.speedPattern[0] = project.speedOverride;
    else song.meta.speedPattern.push(project.speedOverride);
    changed = true;
  }
  if (project.highlightAOverride != null) {
    song.meta.highlightA = project.highlightAOverride;
    changed = true;
  }
  if (project.highlightBOverride != null) {
    song.meta.highlightB = project.highlightBOverride;
    changed = true;
  }
  if (project.virtualTempoOverride != null) {
    song.meta.virtualTempo = project.virtualTempoOverride;
    changed = true;
  }
  if (changed) {
    retime(song);
  }
}
