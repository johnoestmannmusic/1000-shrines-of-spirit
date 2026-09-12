import type { NoteValue, PatternCell } from "./fur/types";
import { cloneCell } from "./songModel";
import { applyEdit, type PatternSnapshot, type SongModel } from "./songModel";

export type EditColumn =
  | { kind: "note" }
  | { kind: "ins" }
  | { kind: "vol" }
  | { kind: "fx"; index: number };

export interface CellPos {
  channel: number;
  order: number;
  row: number;
  column: EditColumn;
}

export type CellValue =
  | { kind: "note"; value: NoteValue | null }
  | { kind: "ins"; value: number | null }
  | { kind: "vol"; value: number | null }
  | { kind: "fx"; value: { effect: number | null; value: number | null } };

export const FX_CATALOG: Array<{ code: number; label: string; description: string }> = [
  { code: 0x01, label: "01xx", description: "Pitch slide up" },
  { code: 0x02, label: "02xx", description: "Pitch slide down" },
  { code: 0x09, label: "09xx", description: "Set Speed 1" },
  { code: 0x0f, label: "0Fxx", description: "Set Speed 2" },
  { code: 0xf0, label: "F0xx", description: "Set tick rate (Hz = xx × 2/5)" },
  { code: 0xc0, label: "C0xx", description: "Set tick rate in Hz (bits 00)" },
  { code: 0xc1, label: "C1xx", description: "Set tick rate in Hz (bits 01)" },
  { code: 0xc2, label: "C2xx", description: "Set tick rate in Hz (bits 10)" },
  { code: 0xc3, label: "C3xx", description: "Set tick rate in Hz (bits 11)" },
  { code: 0xfd, label: "FDxx", description: "Set virtual tempo numerator" },
  { code: 0xfe, label: "FExx", description: "Set virtual tempo denominator" },
];

export const CLIPBOARD_TAG = "LANTERN-PATTERN-CLIP:";

export function columnLabel(column: EditColumn): string {
  switch (column.kind) {
    case "note":
      return "NOTE";
    case "ins":
      return "INS";
    case "vol":
      return "VOL";
    case "fx":
      return "FX";
  }
}

export function sameColumn(a: EditColumn, b: EditColumn): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "fx" && b.kind === "fx") return a.index === b.index;
  return true;
}

export function flatColumnsForChannel(song: SongModel, channel: number): EditColumn[] {
  const effectColumns = Math.max(song.channels[channel]?.effectColumns ?? 1, 1);
  const columns: EditColumn[] = [{ kind: "note" }, { kind: "ins" }, { kind: "vol" }];
  for (let i = 0; i < effectColumns; i++) columns.push({ kind: "fx", index: i });
  return columns;
}

export function columnIndex(song: SongModel, channel: number, column: EditColumn): number {
  const columns = flatColumnsForChannel(song, channel);
  return columns.findIndex((c) => sameColumn(c, column));
}

export function readValue(cell: PatternCell, column: EditColumn): CellValue {
  switch (column.kind) {
    case "note":
      return { kind: "note", value: cell.note ? { ...cell.note } : null };
    case "ins":
      return { kind: "ins", value: cell.instrument };
    case "vol":
      return { kind: "vol", value: cell.volume };
    case "fx": {
      const slot = cell.effects[column.index] ?? { effect: null, value: null };
      return { kind: "fx", value: { effect: slot.effect, value: slot.value } };
    }
  }
}

export function writeValue(cell: PatternCell, column: EditColumn, value: CellValue): PatternCell {
  const next = cloneCell(cell);
  if (column.kind === "note" && value.kind === "note") next.note = value.value ? { ...value.value } : null;
  else if (column.kind === "ins" && value.kind === "ins") next.instrument = value.value;
  else if (column.kind === "vol" && value.kind === "vol") next.volume = value.value;
  else if (column.kind === "fx" && value.kind === "fx") {
    if (next.effects[column.index]) next.effects[column.index] = { ...value.value };
  }
  return next;
}

export function clearValue(cell: PatternCell, column: EditColumn): PatternCell {
  if (column.kind === "note") return writeValue(cell, column, { kind: "note", value: null });
  if (column.kind === "ins") return writeValue(cell, column, { kind: "ins", value: null });
  if (column.kind === "vol") return writeValue(cell, column, { kind: "vol", value: null });
  return writeValue(cell, column, { kind: "fx", value: { effect: null, value: null } });
}

/**
 * "Last value entered" per column type (Note/Ins/Vol/Fx), matching Rust's
 * `last_note`/`last_ins`/`last_vol`/`last_fx` fields: a single global memory
 * per column type, not per-channel or per-column-index.
 */
export interface LastValues {
  note: number;
  ins: number;
  vol: number;
  fx: { effect: number | null; value: number | null };
}

/** Defaults matching `lantern_core::pitch::DEFAULT_ENTRY_NOTE` (C-4) and Rust's `PatternState::default`. */
export function defaultLastValues(): LastValues {
  return { note: 108, ins: 0, vol: 15, fx: { effect: 0, value: 0 } };
}

/** Writes the tracked "last value" for this column's type into a copy of the cell (the `Z` keybind). */
export function applyLastValue(cell: PatternCell, column: EditColumn, last: LastValues): PatternCell {
  switch (column.kind) {
    case "note":
      return writeValue(cell, column, { kind: "note", value: { kind: "note", note: last.note } });
    case "ins":
      return writeValue(cell, column, { kind: "ins", value: last.ins });
    case "vol":
      return writeValue(cell, column, { kind: "vol", value: last.vol });
    case "fx":
      return writeValue(cell, column, { kind: "fx", value: { ...last.fx } });
  }
}

/**
 * Updates the "last value" memory from a just-committed cell, mirroring Rust's
 * `commit_edit`: only real values update the memory (Off/Release/clear do not).
 */
export function recordLastValue(last: LastValues, column: EditColumn, cell: PatternCell): void {
  switch (column.kind) {
    case "note":
      if (cell.note && cell.note.kind === "note") last.note = cell.note.note;
      break;
    case "ins":
      if (cell.instrument !== null) last.ins = cell.instrument;
      break;
    case "vol":
      if (cell.volume !== null) last.vol = cell.volume;
      break;
    case "fx": {
      const slot = cell.effects[column.index];
      if (slot && slot.effect !== null) last.fx = { ...slot };
      break;
    }
  }
}

export function adjustNote(cell: PatternCell, delta: number): PatternCell {
  if (!cell.note || cell.note.kind !== "note") return cell;
  const note = Math.min(Math.max(cell.note.note + delta, 0), 179);
  return writeValue(cell, { kind: "note" }, { kind: "note", value: { kind: "note", note } });
}

export function adjustCell(
  cell: PatternCell,
  column: EditColumn,
  delta: number,
  instrumentCount: number,
): PatternCell {
  if (column.kind === "note") return adjustNote(cell, delta);
  const value = readValue(cell, column);
  if (value.kind === "ins" && value.value !== null) {
    const next = Math.min(Math.max(value.value + delta, 0), Math.max(instrumentCount - 1, 0));
    return writeValue(cell, column, { kind: "ins", value: next });
  }
  if (value.kind === "vol" && value.value !== null) {
    const next = Math.min(Math.max(value.value + delta, 0), 15);
    return writeValue(cell, column, { kind: "vol", value: next });
  }
  if (value.kind === "fx" && value.value.value !== null) {
    const next = Math.min(Math.max(value.value.value + delta, 0), 255);
    return writeValue(cell, column, {
      kind: "fx",
      value: { effect: value.value.effect, value: next },
    });
  }
  return cell;
}

export interface FlatColumn {
  channel: number;
  column: EditColumn;
}

/** Every channel's flat columns in order, matching Rust's global `flat_columns`. */
export function flatColumns(song: SongModel): FlatColumn[] {
  const out: FlatColumn[] = [];
  const channelCount = Math.min(song.channels.length, 4);
  for (let channel = 0; channel < channelCount; channel++) {
    for (const column of flatColumnsForChannel(song, channel)) out.push({ channel, column });
  }
  return out;
}

export function globalColumnIndex(
  song: SongModel,
  channel: number,
  column: EditColumn,
): number {
  return flatColumns(song).findIndex(
    (fc) => fc.channel === channel && sameColumn(fc.column, column),
  );
}

export interface SelectionRect {
  order: number;
  rowLo: number;
  rowHi: number;
  /** Global flat-column indices spanning channels. */
  colLo: number;
  colHi: number;
}

export function columnInRect(
  song: SongModel,
  rect: SelectionRect,
  channel: number,
  column: EditColumn,
): boolean {
  const index = globalColumnIndex(song, channel, column);
  return index >= rect.colLo && index <= rect.colHi;
}

export function selectionRect(
  song: SongModel,
  selected: CellPos | null,
  anchor: CellPos | null,
): SelectionRect | null {
  if (!selected) return null;
  const selectedIndex = globalColumnIndex(song, selected.channel, selected.column);
  if (!anchor || anchor.order !== selected.order) {
    return {
      order: selected.order,
      rowLo: selected.row,
      rowHi: selected.row,
      colLo: selectedIndex,
      colHi: selectedIndex,
    };
  }
  const anchorIndex = globalColumnIndex(song, anchor.channel, anchor.column);
  return {
    order: selected.order,
    rowLo: Math.min(anchor.row, selected.row),
    rowHi: Math.max(anchor.row, selected.row),
    colLo: Math.min(anchorIndex, selectedIndex),
    colHi: Math.max(anchorIndex, selectedIndex),
  };
}

/** Linearly interpolates the selected column's value between its two endpoints. */
export function interpolateColumn(
  song: SongModel,
  channel: number,
  column: EditColumn,
  order: number,
  rowLo: number,
  rowHi: number,
): boolean {
  if (rowHi <= rowLo) return false;
  const first = readValue(cellFrom(song, channel, order, rowLo), column);
  const last = readValue(cellFrom(song, channel, order, rowHi), column);
  const span = rowHi - rowLo;
  let changed = false;
  for (let row = rowLo + 1; row < rowHi; row++) {
    const t = (row - rowLo) / span;
    if (first.kind === "note" && last.kind === "note") {
      const a = first.value && first.value.kind === "note" ? first.value.note : null;
      const b = last.value && last.value.kind === "note" ? last.value.note : null;
      if (a === null || b === null) return false;
      const value = Math.round(a + (b - a) * t);
      applyEdit(song, {
        channel,
        order,
        row,
        cell: writeValue(cellFrom(song, channel, order, row), column, {
          kind: "note",
          value: { kind: "note", note: value },
        }),
      });
      changed = true;
    } else if (first.kind === "ins" && last.kind === "ins") {
      if (first.value === null || last.value === null) return false;
      const value = Math.round(first.value + (last.value - first.value) * t);
      applyEdit(song, {
        channel,
        order,
        row,
        cell: writeValue(cellFrom(song, channel, order, row), column, { kind: "ins", value }),
      });
      changed = true;
    } else if (first.kind === "vol" && last.kind === "vol") {
      if (first.value === null || last.value === null) return false;
      const value = Math.round(first.value + (last.value - first.value) * t);
      applyEdit(song, {
        channel,
        order,
        row,
        cell: writeValue(cellFrom(song, channel, order, row), column, { kind: "vol", value }),
      });
      changed = true;
    } else if (first.kind === "fx" && last.kind === "fx") {
      if (first.value.value === null || last.value.value === null) return false;
      const value = Math.round(first.value.value + (last.value.value - first.value.value) * t);
      applyEdit(song, {
        channel,
        order,
        row,
        cell: writeValue(cellFrom(song, channel, order, row), column, {
          kind: "fx",
          value: { effect: first.value.effect, value },
        }),
      });
      changed = true;
    } else {
      return false;
    }
  }
  return changed;
}

function cellFrom(song: SongModel, channel: number, order: number, row: number): PatternCell {
  const ch = song.channels[channel];
  if (!ch) return { note: null, instrument: null, volume: null, effects: [] };
  const patternIndex = ch.orderList[order];
  if (patternIndex === undefined) return { note: null, instrument: null, volume: null, effects: [] };
  const pattern = ch.patterns.get(patternIndex);
  return pattern?.rows[row] ?? { note: null, instrument: null, volume: null, effects: [] };
}

// ---- Pattern Manager snapshot helpers ----

export function insertPatternAfter(
  snapshot: PatternSnapshot,
  pos: number,
  patternLength: number,
  duplicate: boolean,
): void {
  snapshot.orderLength += 1;
  for (const channel of snapshot.channels) {
    const maxIndex = channel.patterns.reduce((max, [index]) => Math.max(max, index), -1);
    const nextIndex = Math.min(maxIndex + 1, 0xffff);
    let rows: PatternCell[] = Array.from({ length: patternLength }, () => ({
      note: null,
      instrument: null,
      volume: null,
      effects: Array.from({ length: 8 }, () => ({ effect: null, value: null })),
    }));
    if (duplicate) {
      const sourceIndex = channel.orderList[pos];
      const source = channel.patterns.find(([index]) => index === sourceIndex)?.[1];
      if (source) rows = source.map(cloneCell);
    }
    channel.patterns.push([nextIndex, rows]);
    channel.orderList.splice(Math.min(pos + 1, channel.orderList.length), 0, nextIndex);
  }
}

export function removePatternAt(snapshot: PatternSnapshot, pos: number): void {
  if (snapshot.orderLength <= 1) return;
  snapshot.orderLength -= 1;
  for (const channel of snapshot.channels) {
    if (pos < channel.orderList.length) channel.orderList.splice(pos, 1);
  }
}

/**
 * Re-targets every INS cell after instrument `deletedIndex` is removed:
 * references to it are cleared, and higher indices shift down by one.
 */
export function remapInstrumentsAfterDelete(
  snapshot: PatternSnapshot,
  deletedIndex: number,
): void {
  for (const channel of snapshot.channels) {
    for (const [, rows] of channel.patterns) {
      for (const cell of rows) {
        if (cell.instrument === null) continue;
        if (cell.instrument === deletedIndex) cell.instrument = null;
        else if (cell.instrument > deletedIndex) cell.instrument -= 1;
      }
    }
  }
}

/**
 * Re-targets INS cells after instrument `deletedIndex` is removed by moving
 * references to it onto `targetIndex` (given in the pre-deletion indexing);
 * all other higher indices shift down by one as usual.
 */
export function reassignInstrument(
  snapshot: PatternSnapshot,
  deletedIndex: number,
  targetIndex: number,
): void {
  const targetAfterDelete = targetIndex < deletedIndex ? targetIndex : targetIndex - 1;
  for (const channel of snapshot.channels) {
    for (const [, rows] of channel.patterns) {
      for (const cell of rows) {
        if (cell.instrument === null) continue;
        if (cell.instrument === deletedIndex) cell.instrument = targetAfterDelete;
        else if (cell.instrument > deletedIndex) cell.instrument -= 1;
      }
    }
  }
}

export function clearPatternsSnapshot(snapshot: PatternSnapshot, patternLength: number): void {
  snapshot.orderLength = 1;
  for (const channel of snapshot.channels) {
    channel.orderList = [0];
    channel.patterns = [
      [
        0,
        Array.from({ length: patternLength }, () => ({
          note: null,
          instrument: null,
          volume: null,
          effects: Array.from({ length: 8 }, () => ({ effect: null, value: null })),
        })),
      ],
    ];
  }
}
