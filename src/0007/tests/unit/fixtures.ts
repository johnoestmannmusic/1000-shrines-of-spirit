import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const projectRoot = new URL("../../", import.meta.url);

export function projectPath(rel: string): string {
  return fileURLToPath(new URL(rel, projectRoot));
}

export function fixtureBytes(rel: string): Uint8Array {
  return new Uint8Array(readFileSync(projectPath(rel)));
}

export function fixtureText(rel: string): string {
  return readFileSync(projectPath(rel), "utf8");
}
