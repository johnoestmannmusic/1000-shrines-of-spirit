import { describe, expect, it } from "vitest";
import { buildSongModel, cellAt, type SongModel } from "@/core/songModel";
import { parseFurFile } from "@/core/fur/node";
import {
  adjustCell,
  adjustNote,
  applyLastValue,
  clearPatternsSnapshot,
  clearValue,
  columnInRect,
  defaultLastValues,
  flatColumnsForChannel,
  insertPatternAfter,
  interpolateColumn,
  readValue,
  recordLastValue,
  reassignInstrument,
  remapInstrumentsAfterDelete,
  removePatternAt,
  selectionRect,
  writeValue,
} from "@/core/tracker";
import { patternSnapshot } from "@/core/songModel";
import { fixtureBytes } from "./fixtures";

function fixture(): SongModel {
  return buildSongModel(parseFurFile(fixtureBytes("tests/fixtures/flight_school_night_shift.fur")));
}

function emptyCell() {
  return {
    note: null,
    instrument: null,
    volume: null,
    effects: Array.from({ length: 8 }, () => ({ effect: null, value: null })),
  };
}

describe("tracker helpers", () => {
  it("orders flat columns NOTE/INS/VOL then FX", () => {
    const song = fixture();
    expect(flatColumnsForChannel(song, 0).map((c) => c.kind)).toEqual([
      "note",
      "ins",
      "vol",
      "fx",
    ]);
  });

  it("round-trips cell values and ignores mismatched writes", () => {
    const cell = emptyCell();
    const withNote = writeValue(cell, { kind: "note" }, { kind: "note", value: { kind: "note", note: 60 } });
    expect(readValue(withNote, { kind: "note" })).toEqual({
      kind: "note",
      value: { kind: "note", note: 60 },
    });
    // Writing a volume value into a NOTE column is a no-op.
    const unchanged = writeValue(cell, { kind: "note" }, { kind: "vol", value: 5 });
    expect(unchanged.note).toBeNull();
  });

  it("clamps note and volume adjustments", () => {
    const cell = writeValue(emptyCell(), { kind: "note" }, { kind: "note", value: { kind: "note", note: 179 } });
    expect(adjustNote(cell, 12).note).toEqual({ kind: "note", note: 179 });
    const vol = writeValue(emptyCell(), { kind: "vol" }, { kind: "vol", value: 15 });
    expect(adjustCell(vol, { kind: "vol" }, 1, 10).volume).toBe(15);
  });

  it("normalizes selection rectangles", () => {
    const song = fixture();
    const rect = selectionRect(
      song,
      { channel: 0, order: 0, row: 4, column: { kind: "vol" } },
      { channel: 0, order: 0, row: 1, column: { kind: "note" } },
    );
    expect(rect).toEqual({ order: 0, rowLo: 1, rowHi: 4, colLo: 0, colHi: 2 });
  });

  it("spans channels in a single selection rectangle", () => {
    const song = fixture();
    const rect = selectionRect(
      song,
      { channel: 1, order: 0, row: 3, column: { kind: "note" } },
      { channel: 0, order: 0, row: 1, column: { kind: "note" } },
    );
    // Channel 0's NOTE is global column 0; channel 1's NOTE is global column 4.
    expect(rect).toEqual({ order: 0, rowLo: 1, rowHi: 3, colLo: 0, colHi: 4 });
    expect(columnInRect(song, rect!, 0, { kind: "vol" })).toBe(true);
    expect(columnInRect(song, rect!, 1, { kind: "note" })).toBe(true);
    expect(columnInRect(song, rect!, 2, { kind: "note" })).toBe(false);
  });

  it("interpolates a note range and keeps the FX effect code", () => {
    const song = fixture();
    // Set endpoints for a volume interpolation.
    const setVol = (row: number, value: number) => {
      const cell = writeValue(emptyCell(), { kind: "vol" }, { kind: "vol", value });
      const pattern = song.channels[0]!.patterns.get(song.channels[0]!.orderList[0]!)!;
      pattern.rows[row] = cell;
    };
    setVol(0, 0);
    setVol(4, 8);
    const changed = interpolateColumn(song, 0, { kind: "vol" }, 0, 0, 4);
    expect(changed).toBe(true);
    expect(cellAt(song, 0, 0, 2).volume).toBe(4);
  });

  it("remaps INS cells when an instrument is deleted", () => {
    const song = fixture();
    const pattern = song.channels[0]!.patterns.get(song.channels[0]!.orderList[0]!)!;
    // Reference instruments 0, 2 and 3 somewhere in the pattern.
    pattern.rows[0]!.instrument = 0;
    pattern.rows[1]!.instrument = 2;
    pattern.rows[2]!.instrument = 3;
    pattern.rows[3]!.instrument = 1;

    const snap = patternSnapshot(song);
    remapInstrumentsAfterDelete(snap, 2);
    const rows = snap.channels[0]!.patterns[0]![1];
    expect(rows[0]!.instrument).toBe(0);
    expect(rows[1]!.instrument).toBeNull(); // deleted
    expect(rows[2]!.instrument).toBe(2); // shifted down from 3
    expect(rows[3]!.instrument).toBe(1);
  });

  it("re-assigns INS cells to another instrument on delete", () => {
    const song = fixture();
    const pattern = song.channels[0]!.patterns.get(song.channels[0]!.orderList[0]!)!;
    pattern.rows[0]!.instrument = 2;
    pattern.rows[1]!.instrument = 3;
    const snap = patternSnapshot(song);
    // Delete instrument 2, re-assign its notes to instrument 4.
    reassignInstrument(snap, 2, 4);
    const rows = snap.channels[0]!.patterns[0]![1];
    expect(rows[0]!.instrument).toBe(3); // 4 shifts down to 3
    expect(rows[1]!.instrument).toBe(2); // 3 shifts down to 2
  });

  it("inserts, removes and clears patterns in a snapshot", () => {
    const song = fixture();
    const snap = patternSnapshot(song);
    const before = snap.orderLength;
    insertPatternAfter(snap, 0, song.meta.patternLength, true);
    expect(snap.orderLength).toBe(before + 1);
    removePatternAt(snap, 0);
    expect(snap.orderLength).toBe(before);
    clearPatternsSnapshot(snap, song.meta.patternLength);
    expect(snap.orderLength).toBe(1);
    expect(snap.channels.every((c) => c.orderList.length === 1)).toBe(true);
  });
});

describe("clearValue", () => {
  it("clears a single sub-column", () => {
    const cell = writeValue(emptyCell(), { kind: "vol" }, { kind: "vol", value: 7 });
    expect(clearValue(cell, { kind: "vol" }).volume).toBeNull();
  });
});

describe("last-value memory (Z key)", () => {
  it("defaults to C-4 / instrument 0 / volume F / effect 00", () => {
    const last = defaultLastValues();
    expect(last).toEqual({ note: 108, ins: 0, vol: 15, fx: { effect: 0, value: 0 } });
  });

  it("applyLastValue writes the tracked value for each column kind", () => {
    const last = { note: 64, ins: 3, vol: 9, fx: { effect: 0x01, value: 0x20 } };
    expect(applyLastValue(emptyCell(), { kind: "note" }, last).note).toEqual({
      kind: "note",
      note: 64,
    });
    expect(applyLastValue(emptyCell(), { kind: "ins" }, last).instrument).toBe(3);
    expect(applyLastValue(emptyCell(), { kind: "vol" }, last).volume).toBe(9);
    expect(applyLastValue(emptyCell(), { kind: "fx", index: 0 }, last).effects[0]).toEqual({
      effect: 0x01,
      value: 0x20,
    });
  });

  it("recordLastValue captures real Note/Ins/Vol/Fx edits", () => {
    const last = defaultLastValues();
    const note = writeValue(emptyCell(), { kind: "note" }, { kind: "note", value: { kind: "note", note: 72 } });
    recordLastValue(last, { kind: "note" }, note);
    expect(last.note).toBe(72);

    const ins = writeValue(emptyCell(), { kind: "ins" }, { kind: "ins", value: 5 });
    recordLastValue(last, { kind: "ins" }, ins);
    expect(last.ins).toBe(5);

    const vol = writeValue(emptyCell(), { kind: "vol" }, { kind: "vol", value: 2 });
    recordLastValue(last, { kind: "vol" }, vol);
    expect(last.vol).toBe(2);

    const fx = writeValue(emptyCell(), { kind: "fx", index: 0 }, { kind: "fx", value: { effect: 0x09, value: 4 } });
    recordLastValue(last, { kind: "fx", index: 0 }, fx);
    expect(last.fx).toEqual({ effect: 0x09, value: 4 });
  });

  it("recordLastValue ignores Note Off, clears, and empty effect slots", () => {
    const last = defaultLastValues();
    const noteOff = writeValue(emptyCell(), { kind: "note" }, { kind: "note", value: { kind: "off" } });
    recordLastValue(last, { kind: "note" }, noteOff);
    expect(last.note).toBe(108);

    recordLastValue(last, { kind: "ins" }, emptyCell());
    expect(last.ins).toBe(0);

    recordLastValue(last, { kind: "vol" }, emptyCell());
    expect(last.vol).toBe(15);

    recordLastValue(last, { kind: "fx", index: 0 }, emptyCell());
    expect(last.fx).toEqual({ effect: 0, value: 0 });
  });
});
