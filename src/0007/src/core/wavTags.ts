/**
 * Builds RIFF metadata chunks for WAV export: a `LIST`/`INFO` block (widely
 * read by desktop players) and an `id3 ` chunk holding an ID3v2.3 tag with
 * Title/Artist/Album plus an embedded front-cover picture (APIC).
 */

export interface WavTags {
  title?: string;
  artist?: string;
  album?: string;
  /** PNG bytes embedded as the front cover. */
  artwork?: Uint8Array;
}

const utf8 = new TextEncoder();

function pushU32LE(out: number[], value: number): void {
  out.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
}

function pushU32BE(out: number[], value: number): void {
  out.push((value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff);
}

function pushAscii(out: number[], text: string): void {
  for (const ch of text) out.push(ch.charCodeAt(0) & 0xff);
}

/** id + little-endian size + data, padded to an even length. */
function riffChunk(id: string, data: Uint8Array): number[] {
  const out: number[] = [];
  pushAscii(out, id);
  pushU32LE(out, data.length);
  for (const byte of data) out.push(byte);
  if (data.length % 2 === 1) out.push(0);
  return out;
}

/** `LIST`/`INFO` with INAM/IART/IPRD text fields (Latin-1/ASCII). */
function buildInfoChunk(tags: WavTags): number[] {
  const info: number[] = [];
  pushAscii(info, "INFO");
  const sub = (id: string, text: string | undefined) => {
    if (!text) return;
    const bytes = utf8.encode(text);
    const data: number[] = [];
    pushAscii(data, id);
    pushU32LE(data, bytes.length + 1); // + null terminator
    for (const byte of bytes) data.push(byte);
    data.push(0);
    if ((bytes.length + 1) % 2 === 1) data.push(0);
    info.push(...data);
  };
  sub("INAM", tags.title);
  sub("IART", tags.artist);
  sub("IPRD", tags.album);
  return riffChunk("LIST", Uint8Array.from(info));
}

function id3Frame(id: string, payload: Uint8Array): number[] {
  const out: number[] = [];
  pushAscii(out, id);
  pushU32BE(out, payload.length);
  out.push(0, 0); // frame flags
  for (const byte of payload) out.push(byte);
  return out;
}

/** ID3v2.3 `TIT2`/`TPE1`/`TALB` text frames and an `APIC` cover, wrapped as an `id3 ` chunk. */
function buildId3Chunk(tags: WavTags): number[] {
  const frames: number[] = [];
  const textFrame = (id: string, text: string | undefined) => {
    if (!text) return;
    const body = [0x03, ...utf8.encode(text)]; // UTF-8 encoding byte
    frames.push(...id3Frame(id, Uint8Array.from(body)));
  };
  textFrame("TIT2", tags.title);
  textFrame("TPE1", tags.artist);
  textFrame("TALB", tags.album);
  if (tags.artwork && tags.artwork.length > 0) {
    const body: number[] = [0x00]; // Latin-1 for MIME/description
    for (const byte of utf8.encode("image/png")) body.push(byte);
    body.push(0); // MIME terminator
    body.push(0x03); // picture type 3 = front cover
    body.push(0); // empty description terminator
    for (const byte of tags.artwork) body.push(byte);
    frames.push(...id3Frame("APIC", Uint8Array.from(body)));
  }

  const size = frames.length;
  const tag: number[] = [0x49, 0x44, 0x33, 3, 0, 0]; // "ID3" v2.3.0, no flags
  tag.push((size >>> 21) & 0x7f, (size >>> 14) & 0x7f, (size >>> 7) & 0x7f, size & 0x7f);
  tag.push(...frames);
  return riffChunk("id3 ", Uint8Array.from(tag));
}

export function buildWavMetadataChunks(tags: WavTags): Uint8Array {
  return Uint8Array.from([...buildInfoChunk(tags), ...buildId3Chunk(tags)]);
}
