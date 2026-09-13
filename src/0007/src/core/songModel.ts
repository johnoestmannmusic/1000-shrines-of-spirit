import type {
  ChipDef,
  GameBoyParams,
  NoteValue,
  Pattern,
  PatternCell,
  RawFurModule,
  Wavetable,
} from "./fur/types";
import { emptyPatternCell } from "./fur/types";
import { buildRowTiming } from "./timing";

export interface SongMeta {
  name: string;
  author: string;
  system: string;
  tuningA4: number;
  formatVersion: number;
  tickRate: number;
  speedPattern: number[];
  patternLength: number;
  orderLength: number;
  highlightA: number;
  highlightB: number;
  comment: string;
  virtualTempo: [number, number];
}

export interface InstrumentInfo {
  name: string;
  insType: number;
  gameBoy: GameBoyParams | null;
  colorRgb: [number, number, number];
}

export interface Channel {
  index: number;
  effectColumns: number;
  orderList: number[];
  /** Keyed by Furnace pattern index (not guaranteed dense). */
  patterns: Map<number, Pattern>;
  /** insTimeline[order][row] -> instrument index, held across sustains. */
  insTimeline: (number | null)[][];
  /** noteTimeline[order][row] -> held note, held across sustains. */
  noteTimeline: (NoteValue | null)[][];
}

export interface SongModel {
  meta: SongMeta;
  channels: Channel[];
  instruments: InstrumentInfo[];
  wavetables: Wavetable[];
  chips: ChipDef[];
  rowTimes: number[];
  rowTicks: number[];
}

export interface PatternEdit {
  channel: number;
  order: number;
  row: number;
  cell: PatternCell;
}

export interface ChannelPatternSnapshot {
  orderList: number[];
  patterns: Array<[number, PatternCell[]]>;
}

export interface PatternSnapshot {
  orderLength: number;
  channels: ChannelPatternSnapshot[];
}

export function cloneNote(note: NoteValue): NoteValue {
  return { ...note };
}

export function cloneCell(cell: PatternCell): PatternCell {
  return {
    note: cell.note ? cloneNote(cell.note) : null,
    instrument: cell.instrument,
    volume: cell.volume,
    effects: cell.effects.map((e) => ({ effect: e.effect, value: e.value })),
  };
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const ss = s / 100;
  const ll = l / 100;
  const c = (1 - Math.abs(2 * ll - 1)) * ss;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = ll - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return [
    Math.round((r + m) * 255),
    Math.round((g + m) * 255),
    Math.round((b + m) * 255),
  ];
}

function buildInstrumentTimeline(ch: Channel, patternLength: number): (number | null)[][] {
  let current: number | null = null;
  return ch.orderList.map((pIdx) => {
    const pat = ch.patterns.get(pIdx);
    const row: (number | null)[] = [];
    for (let r = 0; r < patternLength; r++) {
      const cell = pat?.rows[r];
      if (cell && cell.instrument !== null) {
        // Furnace keeps the channel's instrument across note-offs; only a new
        // instrument value changes it. (Clearing it on OFF made notes after an
        // OFF play with no instrument, i.e. silently.)
        current = cell.instrument;
      }
      row.push(current);
    }
    return row;
  });
}

function buildNoteTimeline(ch: Channel, patternLength: number): (NoteValue | null)[][] {
  let current: NoteValue | null = null;
  return ch.orderList.map((pIdx) => {
    const pat = ch.patterns.get(pIdx);
    const row: (NoteValue | null)[] = [];
    for (let r = 0; r < patternLength; r++) {
      const cell = pat?.rows[r];
      if (cell && cell.note) {
        current = cell.note.kind === "off" ? null : cell.note;
      }
      row.push(current);
    }
    return row;
  });
}

export function instrumentColor(index: number): [number, number, number] {
  return hslToRgb((index * 137.508) % 360, 65, 55);
}

export function buildSongModel(raw: RawFurModule): SongModel {
  const subsong = raw.subsongs[0]!;

  const channels: Channel[] = [];
  for (let ch = 0; ch < raw.info.totalChannels; ch++) {
    const patterns = new Map<number, Pattern>();
    for (const p of subsong.patterns) {
      if (p.channel === ch) patterns.set(p.index, p);
    }
    const channel: Channel = {
      index: ch,
      effectColumns: subsong.effectColumns[ch] ?? 1,
      orderList: (subsong.orders[ch] ?? []).slice(),
      patterns,
      insTimeline: [],
      noteTimeline: [],
    };
    channel.insTimeline = buildInstrumentTimeline(channel, subsong.patternLength);
    channel.noteTimeline = buildNoteTimeline(channel, subsong.patternLength);
    channels.push(channel);
  }

  const instruments: InstrumentInfo[] = raw.instruments.map((ins, i) => ({
    name: ins.name,
    insType: ins.insType,
    gameBoy: ins.gameBoy,
    colorRgb: hslToRgb((i * 137.508) % 360, 65, 55),
  }));

  const song: SongModel = {
    meta: {
      name: raw.info.name,
      author: raw.info.author,
      system: raw.info.system,
      tuningA4: raw.info.tuningA4,
      formatVersion: raw.formatVersion,
      tickRate: subsong.ticksPerSecond,
      speedPattern: subsong.speedPattern.slice(),
      patternLength: subsong.patternLength,
      orderLength: subsong.orderLength,
      highlightA: subsong.highlightA,
      highlightB: subsong.highlightB,
      comment: subsong.comment,
      virtualTempo: [subsong.virtualTempoNum, subsong.virtualTempoDen],
    },
    channels,
    instruments,
    wavetables: raw.wavetables,
    chips: raw.info.chips,
    rowTimes: [],
    rowTicks: [],
  };

  const timing = buildRowTiming(song);
  song.rowTimes = timing.starts;
  song.rowTicks = timing.ticks;
  return song;
}

/** Minimal project shape needed to reconstruct a song without a `.fur`. */
export interface ProjectSongSource {
  songTitle?: string;
  artist?: string;
  comments?: string;
  tickRateOverride?: number | null;
  speedOverride?: number | null;
  highlightAOverride?: number | null;
  highlightBOverride?: number | null;
  virtualTempoOverride?: [number, number] | null;
  instruments: unknown[];
  patternSnapshot?: PatternSnapshot | null;
}

/**
 * Builds a playable `SongModel` purely from a Project file — used for songs
 * that ship as a project + Source Samples with no Furnace `.fur` (and so no
 * CHIP MODE). Pattern length defaults to 64 rows.
 */
export function buildSongModelFromProject(project: ProjectSongSource): SongModel {
  const snapshot = project.patternSnapshot ?? null;
  const patternLength = 64;
  const orderLength = snapshot?.orderLength ?? 1;
  const channelCount = Math.max(snapshot?.channels.length ?? 4, 1);

  const channels: Channel[] = [];
  for (let ch = 0; ch < channelCount; ch++) {
    const snap = snapshot?.channels[ch];
    const patterns = new Map<number, Pattern>();
    let effectColumns = 1;
    if (snap) {
      for (const [index, rows] of snap.patterns) {
        patterns.set(index, {
          subsong: 0,
          channel: ch,
          index,
          name: "",
          rows: rows.map(cloneCell),
        });
        for (const cell of rows) {
          for (let e = 0; e < cell.effects.length; e++) {
            const slot = cell.effects[e]!;
            if (slot.effect !== null || slot.value !== null) effectColumns = Math.max(effectColumns, e + 1);
          }
        }
      }
    }
    const channel: Channel = {
      index: ch,
      effectColumns,
      orderList: (snap?.orderList ?? [0]).slice(),
      patterns,
      insTimeline: [],
      noteTimeline: [],
    };
    channel.insTimeline = buildInstrumentTimeline(channel, patternLength);
    channel.noteTimeline = buildNoteTimeline(channel, patternLength);
    channels.push(channel);
  }

  const instrumentCount = Math.max(project.instruments.length, 1);
  const instruments: InstrumentInfo[] = Array.from({ length: instrumentCount }, (_, i) => ({
    name: `Instrument ${(i + 1).toString().padStart(2, "0")}`,
    insType: 2,
    gameBoy: null,
    colorRgb: instrumentColor(i),
  }));

  const song: SongModel = {
    meta: {
      name: project.songTitle || "Untitled",
      author: project.artist || "",
      system: "Game Boy",
      tuningA4: 440,
      formatVersion: 0,
      tickRate: project.tickRateOverride ?? 60,
      speedPattern: [project.speedOverride ?? 6],
      patternLength,
      orderLength,
      highlightA: project.highlightAOverride ?? 4,
      highlightB: project.highlightBOverride ?? 16,
      comment: project.comments || "",
      virtualTempo: project.virtualTempoOverride ?? [1, 1],
    },
    channels,
    instruments,
    wavetables: [],
    chips: [{ chipId: 4, channelCount: 4, volume: 1, panning: 0, frontRear: 0 }],
    rowTimes: [],
    rowTicks: [],
  };
  const timing = buildRowTiming(song);
  song.rowTimes = timing.starts;
  song.rowTicks = timing.ticks;
  return song;
}

export function patternSnapshot(song: SongModel): PatternSnapshot {
  return {
    orderLength: song.meta.orderLength,
    channels: song.channels.map((channel) => {
      const patterns: Array<[number, PatternCell[]]> = Array.from(channel.patterns.entries())
        .map(([index, pattern]) => [index, pattern.rows.map(cloneCell)] as [number, PatternCell[]])
        .sort((a, b) => a[0] - b[0]);
      return { orderList: channel.orderList.slice(), patterns };
    }),
  };
}

export function applySnapshot(song: SongModel, snapshot: PatternSnapshot): void {
  song.meta.orderLength = snapshot.orderLength;
  const patternLength = song.meta.patternLength;
  for (let i = 0; i < song.channels.length && i < snapshot.channels.length; i++) {
    const channel = song.channels[i]!;
    const snap = snapshot.channels[i]!;
    channel.orderList = snap.orderList.slice();
    channel.patterns = new Map();
    for (const [index, rows] of snap.patterns) {
      channel.patterns.set(index, {
        subsong: 0,
        channel: channel.index,
        index,
        name: "",
        rows: rows.map(cloneCell),
      });
    }
    channel.insTimeline = buildInstrumentTimeline(channel, patternLength);
    channel.noteTimeline = buildNoteTimeline(channel, patternLength);
  }
  retime(song);
}

export function retime(song: SongModel): void {
  const timing = buildRowTiming(song);
  song.rowTimes = timing.starts;
  song.rowTicks = timing.ticks;
}

export function cellAt(song: SongModel, channel: number, order: number, row: number): PatternCell {
  const ch = song.channels[channel];
  if (!ch) return emptyPatternCell();
  const patternIndex = ch.orderList[order];
  if (patternIndex === undefined) return emptyPatternCell();
  const pattern = ch.patterns.get(patternIndex);
  if (!pattern) return emptyPatternCell();
  const cell = pattern.rows[row];
  if (!cell) return emptyPatternCell();
  return cloneCell(cell);
}

export function applyEdit(song: SongModel, edit: PatternEdit): void {
  const patternLength = song.meta.patternLength;
  const channel = song.channels[edit.channel];
  if (!channel) return;
  const patternIndex = channel.orderList[edit.order];
  if (patternIndex === undefined) return;

  let pattern = channel.patterns.get(patternIndex);
  if (!pattern) {
    pattern = {
      subsong: 0,
      channel: edit.channel,
      index: patternIndex,
      name: "",
      rows: Array.from({ length: patternLength }, () => emptyPatternCell()),
    };
    channel.patterns.set(patternIndex, pattern);
  }
  while (pattern.rows.length <= edit.row) pattern.rows.push(emptyPatternCell());
  pattern.rows[edit.row] = cloneCell(edit.cell);

  channel.insTimeline = buildInstrumentTimeline(channel, patternLength);
  channel.noteTimeline = buildNoteTimeline(channel, patternLength);
  retime(song);
}
