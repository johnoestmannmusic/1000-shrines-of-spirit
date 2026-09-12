import { inflateSync } from "node:zlib";
import { parse } from "./parse";
import type { RawFurModule } from "./types";

/**
 * Node/Electron-main convenience wrapper: zlib-inflates a `.fur` file's bytes
 * when required and parses it. The browser renderer never imports this module
 * (it receives an already-parsed module over IPC), so `node:zlib` stays out of
 * the renderer bundle.
 */
export function parseFurFile(bytes: Uint8Array): RawFurModule {
  return parse(bytes, (data) => new Uint8Array(inflateSync(data)));
}
