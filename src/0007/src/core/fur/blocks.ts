import { FurError } from "./error";
import { Reader, tagEquals, tagToString } from "./reader";
import {
  emptyPatternCell,
  type AssetDir,
  type ChipDef,
  type GameBoyParams,
  type Instrument,
  type NoteValue,
  type Pattern,
  type PatternCell,
  type SongInfo,
  type Subsong,
  type Wavetable,
} from "./types";

export const ELEMENT_TYPE = {
  SUBSONG: 1,
  ASSET_DIR: 3,
  INSTRUMENT: 4,
  WAVETABLE: 5,
  PATTERN: 7,
} as const;

export interface BlockHeader {
  tag: Uint8Array;
  dataStart: number;
  dataEnd: number;
}

export function readBlockHeader(raw: Uint8Array, pos: number): BlockHeader {
  const r = new Reader(raw, pos);
  const tag = r.tag();
  const size = r.u32();
  const dataStart = r.pos;
  return { tag, dataStart, dataEnd: dataStart + size };
}

function expectEnd(r: Reader, dataEnd: number, tag: string): void {
  if (r.pos !== dataEnd) {
    throw new FurError(
      "sizeMismatch",
      `block ${tag}: consumed ${r.pos} bytes but block size is ${dataEnd}`,
      { tag, offset: dataEnd, consumed: r.pos, declared: dataEnd },
    );
  }
}

export interface Inf2Result {
  info: SongInfo;
  elements: Array<{ type: number; pointers: number[] }>;
}

export function parseInf2(r: Reader, dataEnd: number): Inf2Result {
  const name = r.cstr();
  const author = r.cstr();
  const system = r.cstr();
  const category = r.cstr();
  r.cstr(); // name_ja
  r.cstr(); // author_ja
  r.cstr(); // system_ja
  r.cstr(); // category_ja

  const tuningA4 = r.f32();
  r.u8(); // auto_system_name
  const masterVolume = r.f32();
  const totalChannels = r.u16();
  const numChips = r.u16();

  const chips: ChipDef[] = [];
  for (let i = 0; i < numChips; i++) {
    chips.push({
      chipId: r.u16(),
      channelCount: r.u16(),
      volume: r.f32(),
      panning: r.f32(),
      frontRear: r.f32(),
    });
  }

  const patchCount = r.u32();
  for (let i = 0; i < patchCount; i++) r.u32();
  r.u8(); // auto_patchbay

  const elements: Inf2Result["elements"] = [];
  for (;;) {
    const type = r.u8();
    if (type === 0) break;
    const count = r.u32();
    const pointers: number[] = [];
    for (let i = 0; i < count; i++) pointers.push(r.u32());
    elements.push({ type, pointers });
  }

  expectEnd(r, dataEnd, "INF2");

  return {
    info: {
      name,
      author,
      system,
      category,
      tuningA4,
      masterVolume,
      totalChannels,
      chips,
    },
    elements,
  };
}

export function parseSng2(r: Reader, channels: number, dataEnd: number): Subsong {
  const ticksPerSecond = r.f32();
  const initialArpSpeed = r.u8();
  const effectSpeedDivider = r.u8();
  const patternLength = r.u16();
  const orderLength = r.u16();
  const highlightA = r.u8();
  const highlightB = r.u8();
  const virtualTempoNum = r.u16();
  const virtualTempoDen = r.u16();
  r.u8(); // speed_pattern_len (ignored)
  const speedPattern: number[] = [];
  for (let i = 0; i < 16; i++) speedPattern.push(r.u16());

  const name = r.cstr();
  const comment = r.cstr();

  const orders: number[][] = [];
  for (let c = 0; c < channels; c++) orders.push(Array.from(r.bytes(orderLength)));

  const effectColumns = Array.from(r.bytes(channels));
  const channelHidden = Array.from(r.bytes(channels));
  const channelCollapsed = Array.from(r.bytes(channels));

  const channelNames: string[] = [];
  for (let c = 0; c < channels; c++) channelNames.push(r.cstr());
  const channelShortNames: string[] = [];
  for (let c = 0; c < channels; c++) channelShortNames.push(r.cstr());

  const channelColors: number[][] = [];
  for (let c = 0; c < channels; c++) channelColors.push(Array.from(r.bytes(4)));

  expectEnd(r, dataEnd, "SNG2");

  return {
    name,
    comment,
    ticksPerSecond,
    initialArpSpeed,
    effectSpeedDivider,
    patternLength,
    orderLength,
    highlightA,
    highlightB,
    virtualTempoNum,
    virtualTempoDen,
    speedPattern,
    orders,
    effectColumns,
    channelHidden,
    channelCollapsed,
    channelNames,
    channelShortNames,
    channelColors,
    patterns: [],
  };
}

const GAME_BOY_INS_TYPE = 2;

export function parseIns2(r: Reader, dataEnd: number): Instrument {
  r.u16(); // format_version
  const insType = r.u16();

  let name = "";
  let gameBoy: GameBoyParams | null = null;

  while (r.pos < dataEnd) {
    const code = r.bytes(2);
    if (tagEquals(code, "EN")) break;
    const featLen = r.u16();
    const featStart = r.pos;

    if (tagEquals(code, "NA")) {
      name = r.cstr();
    } else if (tagEquals(code, "GB") && insType === GAME_BOY_INS_TYPE) {
      const envelope = r.u8();
      const soundLength = r.u8();
      const flags = r.u8();
      const hwSeqLen = r.u8();
      r.skip(hwSeqLen * 3);
      gameBoy = {
        envelopeVolume: envelope & 0x0f,
        envelopeDirection: ((envelope >> 4) & 0x01) !== 0,
        envelopeLength: (envelope >> 5) & 0x07,
        soundLength,
        softwareEnvelope: (flags & 0x01) !== 0,
        alwaysInit: (flags & 0x02) !== 0,
        doubleWaveWidth: (flags & 0x04) !== 0,
      };
    } else {
      r.skip(featLen);
    }

    const consumed = r.pos - featStart;
    if (consumed < featLen) {
      r.skip(featLen - consumed);
    } else if (consumed > featLen) {
      throw new FurError(
        "sizeMismatch",
        `INS2 feature: consumed ${consumed} bytes but chunk size is ${featLen}`,
        { tag: "INS2 feature", offset: featStart, consumed, declared: featLen },
      );
    }
  }

  expectEnd(r, dataEnd, "INS2");
  return { name, insType, gameBoy };
}

export function parseWave(r: Reader, dataEnd: number): Wavetable {
  const name = r.cstr();
  const width = r.i32();
  r.i32(); // reserved
  const height = r.i32();
  const data: number[] = [];
  for (let i = 0; i < Math.max(width, 0); i++) data.push(r.i32());
  expectEnd(r, dataEnd, "WAVE");
  return { name, width, height, data };
}

const NOTE_OFF = 180;
const NOTE_RELEASE = 181;
const NOTE_MACRO_RELEASE = 182;
const NOTE_RAW_FREQ = 183;

function decodeNote(v: number, r: Reader): NoteValue {
  switch (v) {
    case NOTE_OFF:
      return { kind: "off" };
    case NOTE_RELEASE:
      return { kind: "release" };
    case NOTE_MACRO_RELEASE:
      return { kind: "macroRelease" };
    case NOTE_RAW_FREQ:
      return { kind: "rawFreq", value: r.u32() };
    default:
      return { kind: "note", note: v };
  }
}

export function parsePatn(r: Reader, patternLength: number, dataEnd: number): Pattern {
  const subsong = r.u8();
  const channel = r.u8();
  const index = r.u16();
  const name = r.cstr();

  const rows: PatternCell[] = Array.from({ length: patternLength }, () => emptyPatternCell());
  let rowI = 0;

  while (r.pos < dataEnd) {
    const b0 = r.u8();
    if (b0 === 0xff) break;
    if ((b0 & 0x80) !== 0) {
      rowI += (b0 & 0x7f) + 2;
      continue;
    }
    if (b0 === 0) {
      rowI += 1;
      continue;
    }

    const cell = emptyPatternCell();
    if (b0 & 0x01) cell.note = decodeNote(r.u8(), r);
    if (b0 & 0x02) cell.instrument = r.u8();
    if (b0 & 0x04) cell.volume = r.u8();
    if (b0 & 0x08) cell.effects[0].effect = r.u8();
    if (b0 & 0x10) cell.effects[0].value = r.u8();
    if (b0 & 0x20) {
      const b1 = r.u8();
      if (b1 & 0x01) cell.effects[1].effect = r.u8();
      if (b1 & 0x02) cell.effects[1].value = r.u8();
      if (b1 & 0x04) cell.effects[2].effect = r.u8();
      if (b1 & 0x08) cell.effects[2].value = r.u8();
      if (b1 & 0x10) cell.effects[3].effect = r.u8();
      if (b1 & 0x20) cell.effects[3].value = r.u8();
    }
    if (b0 & 0x40) {
      const b2 = r.u8();
      if (b2 & 0x01) cell.effects[4].effect = r.u8();
      if (b2 & 0x02) cell.effects[4].value = r.u8();
      if (b2 & 0x04) cell.effects[5].effect = r.u8();
      if (b2 & 0x08) cell.effects[5].value = r.u8();
      if (b2 & 0x10) cell.effects[6].effect = r.u8();
      if (b2 & 0x20) cell.effects[6].value = r.u8();
      if (b2 & 0x40) cell.effects[7].effect = r.u8();
      if (b2 & 0x80) cell.effects[7].value = r.u8();
    }

    if (rowI >= 0 && rowI < rows.length) rows[rowI] = cell;
    rowI += 1;
  }

  expectEnd(r, dataEnd, "PATN");
  return { subsong, channel, index, name, rows };
}

export function parseAdir(r: Reader, dataEnd: number): AssetDir[] {
  const ndirs = r.u32();
  const dirs: AssetDir[] = [];
  for (let i = 0; i < ndirs; i++) {
    const name = r.cstr();
    const nassets = r.u16();
    const assets = Array.from(r.bytes(nassets));
    dirs.push({ name, assets });
  }
  expectEnd(r, dataEnd, "ADIR");
  return dirs;
}

export interface OldInfoResult {
  info: SongInfo;
  subsong: Subsong;
  instrumentPtrs: number[];
  wavetablePtrs: number[];
  patternPtrs: number[];
}

function readPtrs(r: Reader, count: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < count; i++) out.push(r.u32());
  return out;
}

function readStrings(r: Reader, count: number): string[] {
  const out: string[] = [];
  for (let i = 0; i < count; i++) out.push(r.cstr());
  return out;
}

export function parseOldInfo(
  r: Reader,
  formatVersion: number,
  dataEnd: number,
): OldInfoResult {
  if (formatVersion < 157) {
    throw new FurError(
      "unsupportedFormat",
      `Furnace format version ${formatVersion} is too old for this port (requires PATN/INS2)`,
      formatVersion,
    );
  }
  const timeBase = r.u8() + 1;
  const speed1 = r.u8();
  const speed2 = r.u8();
  const initialArpSpeed = r.u8();
  const ticksPerSecond = r.f32();
  const patternLength = r.u16();
  const orderLength = r.u16();
  const highlightA = r.u8();
  const highlightB = r.u8();
  const instrumentCount = r.u16();
  const wavetableCount = r.u16();
  const sampleCount = r.u16();
  const patternCount = r.u32();

  const chipIds = Array.from(r.bytes(32));
  const oldVolumes = Array.from(r.bytes(32));
  const oldPans = Array.from(r.bytes(32));
  r.skip(32 * 4);
  const activeIds: number[] = [];
  for (const id of chipIds) {
    if (id === 0) break;
    activeIds.push(id);
  }
  if (!(activeIds.length === 1 && activeIds[0] === 4)) {
    throw new FurError(
      "unsupportedInstrumentType",
      `old INFO active chip ${activeIds[0] ?? 0} is not supported (Game Boy only)`,
      activeIds[0] ?? 0,
    );
  }
  const channels = 4;

  const name = r.cstr();
  const author = r.cstr();
  const tuningA4 = r.f32();
  r.skip(20);

  const instrumentPtrs = readPtrs(r, instrumentCount);
  const wavetablePtrs = readPtrs(r, wavetableCount);
  r.skip(sampleCount * 4);
  const patternPtrs = readPtrs(r, patternCount);

  const orders: number[][] = [];
  for (let c = 0; c < channels; c++) orders.push(Array.from(r.bytes(orderLength)));
  const effectColumns = Array.from(r.bytes(channels));
  const channelHidden = Array.from(r.bytes(channels));
  const channelCollapsed = Array.from(r.bytes(channels));
  const channelNames = readStrings(r, channels);
  const channelShortNames = readStrings(r, channels);
  const songComment = r.cstr();
  const masterVolume = r.f32();

  r.skip(28);
  const virtualTempoNum = Math.max(r.u16(), 1);
  const virtualTempoDen = Math.max(r.u16(), 1);
  const subsongName = r.cstr();
  const subsongComment = r.cstr();
  const extraSubsongs = r.u8();
  r.skip(3 + extraSubsongs * 4);

  const system = r.cstr();
  const category = r.cstr();
  for (let i = 0; i < 4; i++) r.cstr();

  const chips: ChipDef[] = [];
  for (let index = 0; index < activeIds.length; index++) {
    const id = activeIds[index] as number;
    let volume: number;
    let panning: number;
    let frontRear: number;
    try {
      volume = r.f32();
    } catch {
      volume = ((oldVolumes[index] << 24) >> 24) / 64.0;
    }
    try {
      panning = r.f32();
    } catch {
      panning = ((oldPans[index] << 24) >> 24) / 127.0;
    }
    try {
      frontRear = r.f32();
    } catch {
      frontRear = 0.0;
    }
    chips.push({
      chipId: id,
      channelCount: id === 4 ? 4 : 0,
      volume,
      panning,
      frontRear,
    });
  }
  const patchCount = r.u32();
  r.skip(patchCount * 4);
  r.skip(1);
  r.skip(8);

  const speedLen = r.u8();
  if (speedLen < 1 || speedLen > 16) {
    throw new FurError("invalidSpeedPattern", `invalid speed-pattern length ${speedLen}`, speedLen);
  }
  const speedBytes = r.bytes(16);
  let speedPattern = Array.from(speedBytes.subarray(0, speedLen)).map((s) => s * timeBase);
  if (speedPattern.length === 0) speedPattern = [speed1 * timeBase, speed2 * timeBase];
  const grooveCount = r.u8();
  r.skip(grooveCount * 17);
  r.skip(12);

  expectEnd(r, dataEnd, "INFO");

  const subsong: Subsong = {
    name: subsongName,
    comment: subsongComment.length > 0 ? subsongComment : songComment,
    ticksPerSecond,
    initialArpSpeed,
    effectSpeedDivider: 1,
    patternLength,
    orderLength,
    highlightA,
    highlightB,
    virtualTempoNum,
    virtualTempoDen,
    speedPattern,
    orders,
    effectColumns,
    channelHidden,
    channelCollapsed,
    channelNames,
    channelShortNames,
    channelColors: Array.from({ length: channels }, () => [0, 0, 0, 0]),
    patterns: [],
  };

  return {
    info: {
      name,
      author,
      system,
      category,
      tuningA4,
      masterVolume,
      totalChannels: channels,
      chips,
    },
    subsong,
    instrumentPtrs,
    wavetablePtrs,
    patternPtrs,
  };
}
