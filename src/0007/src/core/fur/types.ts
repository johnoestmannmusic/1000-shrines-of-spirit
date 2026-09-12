// Parsed representation of a Furnace (.fur) module, scoped to Game Boy chip
// modules. Mirrors lantern-fur/src/model.rs so downstream song-model code can
// consume it identically regardless of source.

/** Semitone index from C-(-5) (0) up to B-9 (179), plus the non-note markers. */
export type NoteValue =
  | { kind: "note"; note: number }
  | { kind: "off" }
  | { kind: "release" }
  | { kind: "macroRelease" }
  | { kind: "rawFreq"; value: number };

export interface EffectSlot {
  effect: number | null;
  value: number | null;
}

export interface PatternCell {
  note: NoteValue | null;
  instrument: number | null;
  volume: number | null;
  /** Up to 8 effect columns, in Furnace's internal order. */
  effects: EffectSlot[];
}

export interface Pattern {
  subsong: number;
  channel: number;
  index: number;
  name: string;
  rows: PatternCell[];
}

export interface GameBoyParams {
  envelopeVolume: number;
  envelopeDirection: boolean;
  envelopeLength: number;
  soundLength: number;
  softwareEnvelope: boolean;
  alwaysInit: boolean;
  doubleWaveWidth: boolean;
}

export interface Instrument {
  name: string;
  insType: number;
  gameBoy: GameBoyParams | null;
}

export interface Wavetable {
  name: string;
  width: number;
  height: number;
  data: number[];
}

export interface ChipDef {
  chipId: number;
  channelCount: number;
  volume: number;
  panning: number;
  frontRear: number;
}

export interface SongInfo {
  name: string;
  author: string;
  system: string;
  category: string;
  tuningA4: number;
  masterVolume: number;
  totalChannels: number;
  chips: ChipDef[];
}

export interface Subsong {
  name: string;
  comment: string;
  ticksPerSecond: number;
  initialArpSpeed: number;
  effectSpeedDivider: number;
  patternLength: number;
  orderLength: number;
  highlightA: number;
  highlightB: number;
  virtualTempoNum: number;
  virtualTempoDen: number;
  speedPattern: number[];
  /** orders[channel][order_row] = pattern_index */
  orders: number[][];
  effectColumns: number[];
  channelHidden: number[];
  channelCollapsed: number[];
  channelNames: string[];
  channelShortNames: string[];
  /** ABGR bytes per channel, as stored. */
  channelColors: number[][];
  patterns: Pattern[];
}

export interface RawFurModule {
  formatVersion: number;
  info: SongInfo;
  instruments: Instrument[];
  wavetables: Wavetable[];
  subsongs: Subsong[];
}

export interface AssetDir {
  name: string;
  assets: number[];
}

export const GAME_BOY_CHIP_ID = 4;

export function emptyEffectSlot(): EffectSlot {
  return { effect: null, value: null };
}

export function emptyPatternCell(columnCount = 8): PatternCell {
  return {
    note: null,
    instrument: null,
    volume: null,
    effects: Array.from({ length: columnCount }, emptyEffectSlot),
  };
}
