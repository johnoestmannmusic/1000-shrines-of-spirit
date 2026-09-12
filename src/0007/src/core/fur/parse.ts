import {
  ELEMENT_TYPE,
  parseInf2,
  parseIns2,
  parseOldInfo,
  parsePatn,
  parseSng2,
  parseWave,
  readBlockHeader,
} from "./blocks";
import { FurError } from "./error";
import { Reader, tagEquals, tagToString } from "./reader";
import { GAME_BOY_CHIP_ID, type Instrument, type RawFurModule, type Subsong, type Wavetable } from "./types";

export const MAGIC_TEXT = "-Furnace module-";

export type InflateFn = (data: Uint8Array) => Uint8Array;

export function startsWithMagic(bytes: Uint8Array): boolean {
  if (bytes.length < MAGIC_TEXT.length) return false;
  for (let i = 0; i < MAGIC_TEXT.length; i++) {
    if (bytes[i] !== MAGIC_TEXT.charCodeAt(i)) return false;
  }
  return true;
}

/** Unwraps a `.fur` file's outer zlib compression if present. */
export function unwrap(bytes: Uint8Array, inflate?: InflateFn): Uint8Array {
  if (startsWithMagic(bytes)) return bytes;
  if (!inflate) {
    throw new FurError(
      "zlib",
      "file is zlib-compressed but no inflater was provided to parse()",
    );
  }
  try {
    return inflate(bytes);
  } catch (e) {
    throw new FurError("zlib", `zlib decompression failed: ${String(e)}`);
  }
}

export interface ModuleHeader {
  formatVersion: number;
  songInfoPtr: number;
}

export function readHeader(raw: Uint8Array): ModuleHeader {
  if (!startsWithMagic(raw)) {
    throw new FurError("badMagic", "not a Furnace module (bad magic header)");
  }
  const r = new Reader(raw, MAGIC_TEXT.length);
  const formatVersion = r.u16();
  r.skip(2); // reserved
  const songInfoPtr = r.u32();
  r.skip(8); // reserved
  return { formatVersion, songInfoPtr };
}

function elementPtrs(
  elements: Array<{ type: number; pointers: number[] }>,
  type: number,
): number[] {
  const found = elements.find((e) => e.type === type);
  return found ? found.pointers : [];
}

/** Parses a `.fur` file's bytes (zlib-wrapped or raw) into a RawFurModule. */
export function parse(bytes: Uint8Array, inflate?: InflateFn): RawFurModule {
  const raw = unwrap(bytes, inflate);
  const header = readHeader(raw);

  const block = readBlockHeader(raw, header.songInfoPtr);
  if (tagEquals(block.tag, "INFO")) {
    return parseLegacy(raw, header.formatVersion, block.dataStart, block.dataEnd);
  }
  if (!tagEquals(block.tag, "INF2")) {
    throw new FurError(
      "badSongInfoPointer",
      `song info pointer ${header.songInfoPtr} does not point to an INF2/INFO block`,
      header.songInfoPtr,
    );
  }

  const inf2 = parseInf2(new Reader(raw, block.dataStart), block.dataEnd);

  if (!inf2.info.chips.some((c) => c.chipId === GAME_BOY_CHIP_ID)) {
    throw new FurError(
      "unsupportedInstrumentType",
      `instrument type ${inf2.info.chips[0]?.chipId ?? 0} is not supported (only Game Boy / type 2)`,
      inf2.info.chips[0]?.chipId ?? 0,
    );
  }

  const channels = inf2.info.totalChannels;

  const subsongs: Subsong[] = [];
  for (const ptr of elementPtrs(inf2.elements, ELEMENT_TYPE.SUBSONG)) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "SNG2")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected SNG2)`,
        { offset: ptr, expected: "SNG2", found: tagToString(b.tag) },
      );
    }
    subsongs.push(parseSng2(new Reader(raw, b.dataStart), channels, b.dataEnd));
  }

  const instruments: Instrument[] = [];
  for (const ptr of elementPtrs(inf2.elements, ELEMENT_TYPE.INSTRUMENT)) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "INS2")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected INS2)`,
        { offset: ptr, expected: "INS2", found: tagToString(b.tag) },
      );
    }
    instruments.push(parseIns2(new Reader(raw, b.dataStart), b.dataEnd));
  }

  const wavetables: Wavetable[] = [];
  for (const ptr of elementPtrs(inf2.elements, ELEMENT_TYPE.WAVETABLE)) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "WAVE")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected WAVE)`,
        { offset: ptr, expected: "WAVE", found: tagToString(b.tag) },
      );
    }
    wavetables.push(parseWave(new Reader(raw, b.dataStart), b.dataEnd));
  }

  for (const ptr of elementPtrs(inf2.elements, ELEMENT_TYPE.PATTERN)) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "PATN")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected PATN)`,
        { offset: ptr, expected: "PATN", found: tagToString(b.tag) },
      );
    }
    const subsongIdx = raw[b.dataStart];
    if (subsongIdx === undefined) {
      throw FurError.eof(b.dataStart, 1, raw.length);
    }
    const patternLength = subsongs[subsongIdx]?.patternLength ?? 0;
    const pattern = parsePatn(new Reader(raw, b.dataStart), patternLength, b.dataEnd);
    const target = subsongs[subsongIdx];
    if (target) target.patterns.push(pattern);
  }

  return {
    formatVersion: header.formatVersion,
    info: inf2.info,
    instruments,
    wavetables,
    subsongs,
  };
}

function parseLegacy(
  raw: Uint8Array,
  formatVersion: number,
  dataStart: number,
  dataEnd: number,
): RawFurModule {
  const legacy = parseOldInfo(new Reader(raw, dataStart), formatVersion, dataEnd);
  const subsong = legacy.subsong;

  const instruments: Instrument[] = [];
  for (const ptr of legacy.instrumentPtrs) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "INS2")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected INS2)`,
        { offset: ptr, expected: "INS2", found: tagToString(b.tag) },
      );
    }
    instruments.push(parseIns2(new Reader(raw, b.dataStart), b.dataEnd));
  }

  const wavetables: Wavetable[] = [];
  for (const ptr of legacy.wavetablePtrs) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "WAVE")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected WAVE)`,
        { offset: ptr, expected: "WAVE", found: tagToString(b.tag) },
      );
    }
    wavetables.push(parseWave(new Reader(raw, b.dataStart), b.dataEnd));
  }

  for (const ptr of legacy.patternPtrs) {
    const b = readBlockHeader(raw, ptr);
    if (!tagEquals(b.tag, "PATN")) {
      throw new FurError(
        "unexpectedTag",
        `unexpected block tag ${tagToString(b.tag)} at offset ${ptr} (expected PATN)`,
        { offset: ptr, expected: "PATN", found: tagToString(b.tag) },
      );
    }
    subsong.patterns.push(
      parsePatn(new Reader(raw, b.dataStart), subsong.patternLength, b.dataEnd),
    );
  }

  return {
    formatVersion,
    info: legacy.info,
    instruments,
    wavetables,
    subsongs: [subsong],
  };
}
