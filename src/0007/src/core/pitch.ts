import type { NoteValue } from "./fur/types";

const NOTE_NAMES = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];

/** Furnace's raw byte for its displayed "A-5" (scientific A4, 440 Hz). */
export const A_REF_NOTE = 129;

/** Default note EDIT MODE starts a freshly entered note at. */
export const DEFAULT_ENTRY_NOTE = 108;

export function noteToName(note: NoteValue): string {
  switch (note.kind) {
    case "note": {
      const rel = note.note - 60;
      const semi = ((rel % 12) + 12) % 12;
      const oct = Math.floor(rel / 12);
      return `${NOTE_NAMES[semi]}${oct}`;
    }
    case "off":
      return "OFF";
    case "release":
      return "REL";
    case "macroRelease":
      return "MREL";
    case "rawFreq":
      return "FRQ";
  }
}

/** Frequency in Hz for a note, or null for note-off/release/macro-release. */
export function noteToFreq(note: NoteValue, tuning: number): number | null {
  switch (note.kind) {
    case "note":
      return tuning * Math.pow(2, (note.note - A_REF_NOTE) / 12);
    case "rawFreq":
      return note.value;
    default:
      return null;
  }
}

/** Sampler playback rate for a pattern note + an instrument transpose. */
export function samplerPlaybackRate(
  note: NoteValue,
  tuning: number,
  transpose: number,
): number | null {
  const freq = noteToFreq(note, tuning);
  if (freq === null) return null;
  const refFreq = noteToFreq({ kind: "note", note: A_REF_NOTE }, tuning);
  if (refFreq === null || refFreq === 0) return null;
  return (freq / refFreq) * Math.pow(2, transpose / 12);
}
