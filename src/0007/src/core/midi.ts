import type { NoteValue } from "./fur/types";
import { cellAt, type SongModel } from "./songModel";

const TICKS_PER_ROW = 24;

function pushVarlen(out: number[], value: number): void {
  const bytes = [value & 0x7f];
  let v = value >>> 7;
  while (v > 0) {
    bytes.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  for (let i = bytes.length - 1; i >= 0; i--) out.push(bytes[i]!);
}

function pushDelta(out: number[], lastTick: { value: number }, tick: number): void {
  pushVarlen(out, Math.max(tick - lastTick.value, 0));
  lastTick.value = tick;
}

function pushU16Be(out: number[], value: number): void {
  out.push((value >>> 8) & 0xff, value & 0xff);
}

function pushU32Be(out: number[], value: number): void {
  out.push((value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff);
}

function pushMetaText(
  out: number[],
  lastTick: { value: number },
  tick: number,
  kind: number,
  text: string,
): void {
  pushDelta(out, lastTick, tick);
  out.push(0xff, kind);
  pushVarlen(out, text.length);
  for (const ch of text) out.push(ch.charCodeAt(0) & 0xff);
}

function rowMicrosPerQuarter(seconds: number): number {
  const micros = Math.round(Math.max(0, seconds) * 1_000_000);
  return Math.min(Math.max(micros, 1), 0x00ff_ffff);
}

function buildTempoTrack(song: SongModel): number[] {
  const out: number[] = [];
  const lastTick = { value: 0 };
  const rowCount = song.meta.orderLength * song.meta.patternLength;
  pushMetaText(out, lastTick, 0, 0x03, song.meta.name);

  let lastMicros = -1;
  for (let row = 0; row < rowCount; row++) {
    const start = song.rowTimes[row] ?? 0;
    const end = song.rowTimes[row + 1] ?? start;
    const micros = rowMicrosPerQuarter(end - start);
    if (micros !== lastMicros) {
      pushDelta(out, lastTick, row * TICKS_PER_ROW);
      out.push(0xff, 0x51, 0x03);
      out.push((micros >>> 16) & 0xff, (micros >>> 8) & 0xff, micros & 0xff);
      lastMicros = micros;
    }
  }
  pushDelta(out, lastTick, rowCount * TICKS_PER_ROW);
  out.push(0xff, 0x2f, 0x00);
  return out;
}

function pushNoteOff(
  out: number[],
  lastTick: { value: number },
  channel: number,
  note: number,
  tick: number,
): void {
  pushDelta(out, lastTick, tick);
  out.push(0x80 | channel, note, 0);
}

function noteToMidi(note: NoteValue): number | null {
  if (note.kind !== "note") return null;
  return Math.min(Math.max(note.note - 60, 0), 127);
}

function buildChannelTrack(song: SongModel, channel: number): number[] {
  const out: number[] = [];
  const lastTick = { value: 0 };
  const midiChannel = Math.min(channel, 15);
  const patternLength = Math.max(song.meta.patternLength, 1);
  const rowCount = song.meta.orderLength * patternLength;
  pushMetaText(out, lastTick, 0, 0x03, `CH${channel}`);

  let sounding: number | null = null;
  let lastNoteOnTick = 0;
  for (let row = 0; row < rowCount; row++) {
    const order = Math.floor(row / patternLength);
    const rowInPattern = row % patternLength;
    const cell = cellAt(song, channel, order, rowInPattern);
    const tick = row * TICKS_PER_ROW;
    const note = cell.note;
    if (note) {
      if (note.kind === "note") {
        if (sounding !== null) pushNoteOff(out, lastTick, midiChannel, sounding, tick);
        const midiNote = noteToMidi(note)!;
        const velocity =
          cell.volume === null
            ? 100
            : Math.max(Math.floor((Math.min(cell.volume, 15) * 127) / 15), 1);
        pushDelta(out, lastTick, tick);
        out.push(0x90 | midiChannel, midiNote, velocity);
        sounding = midiNote;
        lastNoteOnTick = tick;
      } else if (
        sounding !== null &&
        (note.kind === "off" || note.kind === "release" || note.kind === "macroRelease")
      ) {
        pushNoteOff(out, lastTick, midiChannel, sounding, tick);
        sounding = null;
      }
    }
  }
  if (sounding !== null) {
    pushNoteOff(out, lastTick, midiChannel, sounding, rowCount * TICKS_PER_ROW);
  }
  pushDelta(out, lastTick, rowCount * TICKS_PER_ROW);
  out.push(0xff, 0x2f, 0x00);
  void lastNoteOnTick;
  return out;
}

/** Writes a Format-1 Standard MIDI File (one tempo track + up to 4 channels). */
export function writeMidi(song: SongModel): Uint8Array {
  const channelCount = Math.min(song.channels.length, 4);
  const tracks = [buildTempoTrack(song)];
  for (let c = 0; c < channelCount; c++) tracks.push(buildChannelTrack(song, c));

  const out: number[] = [];
  for (const ch of "MThd") out.push(ch.charCodeAt(0));
  pushU32Be(out, 6);
  pushU16Be(out, 1);
  pushU16Be(out, tracks.length);
  pushU16Be(out, TICKS_PER_ROW);

  for (const track of tracks) {
    for (const ch of "MTrk") out.push(ch.charCodeAt(0));
    pushU32Be(out, track.length);
    for (const byte of track) out.push(byte);
  }
  return Uint8Array.from(out);
}
