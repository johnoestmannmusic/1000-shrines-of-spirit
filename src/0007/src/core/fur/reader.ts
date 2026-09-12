import { FurError } from "./error";

const utf8 = new TextDecoder("utf-8", { fatal: false });

/** Bounds-checked little-endian byte cursor used by every block parser. */
export class Reader {
  pos: number;
  private readonly view: DataView;

  constructor(
    public readonly buf: Uint8Array,
    pos = 0,
  ) {
    this.pos = pos;
    this.view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  }

  remaining(): number {
    return Math.max(0, this.buf.length - this.pos);
  }

  private need(n: number): void {
    if (this.pos + n > this.buf.length) {
      throw FurError.eof(this.pos, n, this.buf.length);
    }
  }

  u8(): number {
    this.need(1);
    const v = this.buf[this.pos] as number;
    this.pos += 1;
    return v;
  }

  u16(): number {
    this.need(2);
    const v = this.view.getUint16(this.pos, true);
    this.pos += 2;
    return v;
  }

  u32(): number {
    this.need(4);
    const v = this.view.getUint32(this.pos, true);
    this.pos += 4;
    return v;
  }

  i32(): number {
    this.need(4);
    const v = this.view.getInt32(this.pos, true);
    this.pos += 4;
    return v;
  }

  f32(): number {
    this.need(4);
    const v = this.view.getFloat32(this.pos, true);
    this.pos += 4;
    return v;
  }

  bytes(n: number): Uint8Array {
    this.need(n);
    const s = this.buf.subarray(this.pos, this.pos + n);
    this.pos += n;
    return s;
  }

  skip(n: number): void {
    this.need(n);
    this.pos += n;
  }

  /** Reads a zero-terminated UTF-8 string (Furnace's STR field type). */
  cstr(): string {
    const start = this.pos;
    let end = -1;
    for (let i = this.pos; i < this.buf.length; i++) {
      if (this.buf[i] === 0) {
        end = i;
        break;
      }
    }
    if (end < 0) {
      throw FurError.eof(this.pos, 1, this.buf.length);
    }
    const s = utf8.decode(this.buf.subarray(start, end));
    this.pos = end + 1;
    return s;
  }

  /** Reads a 4-byte block tag without comparison. */
  tag(): Uint8Array {
    return this.bytes(4);
  }
}

export function tagEquals(tag: Uint8Array, text: string): boolean {
  if (tag.length !== text.length) return false;
  for (let i = 0; i < tag.length; i++) {
    if (tag[i] !== text.charCodeAt(i)) return false;
  }
  return true;
}

export function tagToString(tag: Uint8Array): string {
  return String.fromCharCode(tag[0] ?? 0, tag[1] ?? 0, tag[2] ?? 0, tag[3] ?? 0);
}
