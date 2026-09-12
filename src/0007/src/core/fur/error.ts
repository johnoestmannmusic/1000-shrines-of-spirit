export type FurErrorCode =
  | "badMagic"
  | "zlib"
  | "unexpectedEof"
  | "unexpectedTag"
  | "sizeMismatch"
  | "unsupportedInstrumentType"
  | "badSongInfoPointer"
  | "unsupportedFormat"
  | "invalidSpeedPattern";

export interface UnexpectedEofDetails {
  pos: number;
  needed: number;
  len: number;
}

export class FurError extends Error {
  constructor(
    public readonly code: FurErrorCode,
    message: string,
    public readonly details?: unknown,
  ) {
    super(message);
    this.name = "FurError";
  }

  static eof(pos: number, needed: number, len: number): FurError {
    const d: UnexpectedEofDetails = { pos, needed, len };
    return new FurError(
      "unexpectedEof",
      `unexpected end of file at ${pos} (needed ${needed} bytes, file length ${len})`,
      d,
    );
  }
}
