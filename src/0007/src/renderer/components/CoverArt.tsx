import { useEffect, useRef } from "react";
import type { SongModel } from "@/core/songModel";
import { songPositionAt } from "@/core/timing";
import type { AudioBackend } from "@/audio/backend";
import { safeFilename } from "../util";
import { saveFile } from "../platform";
import { useExplainer } from "../explainer";
import { coverArtExplain } from "../explainerContent";

const GRID = 32;
const DISC_R = 13;
const TAU = Math.PI * 2;
const BAYER = [
  0.03125, 0.53125, 0.15625, 0.65625, 0.78125, 0.28125, 0.90625, 0.40625, 0.21875, 0.71875,
  0.09375, 0.59375, 0.96875, 0.46875, 0.84375, 0.34375,
];
const CHANNEL_ANGLES = [-Math.PI / 2, 0, Math.PI / 2, Math.PI];

interface Pulse {
  angle: number;
  age: number;
  maxAge: number;
  color: [number, number, number];
  width: number;
}

interface CoverState {
  time: number;
  last: number;
  overall: number;
  phases: Float32Array;
  speeds: Float32Array;
  pulses: Pulse[];
  lastPosition: string | null;
}

function randomWithSeed(seed: number): number {
  const s = (seed + 0x6d2b79f5) >>> 0;
  let v = s;
  v = Math.imul(v ^ (v >>> 15), v | 1) >>> 0;
  v = (v ^ (v + Math.imul(v ^ (v >>> 7), v | 61))) >>> 0;
  return ((v ^ (v >>> 14)) >>> 0) / 4294967296;
}

function makeRandom(): () => number {
  let seed = 9001 >>> 0;
  return () => {
    seed = (seed + 0x6d2b79f5) >>> 0;
    let v = seed;
    v = Math.imul(v ^ (v >>> 15), v | 1) >>> 0;
    v = (v ^ (v + Math.imul(v ^ (v >>> 7), v | 61))) >>> 0;
    return ((v ^ (v >>> 14)) >>> 0) / 4294967296;
  };
}

function wrapAngle(angle: number): number {
  let a = angle % TAU;
  if (a > Math.PI) a -= TAU;
  if (a <= -Math.PI) a += TAU;
  return a;
}

function dither(value: number, x: number, y: number): number {
  const v = Math.min(Math.max(value, 0), 255);
  const step = 255 / 8;
  const scaled = v / step;
  const base = Math.floor(scaled);
  const level = base + (scaled - base > BAYER[(y & 3) * 4 + (x & 3)]! ? 1 : 0);
  return Math.round(Math.min(level, 8) * step);
}

function blend(buffer: Float32Array, x: number, y: number, color: number[], amount: number): void {
  if (x < 0 || y < 0 || x >= GRID || y >= GRID) return;
  const index = (y * GRID + x) * 3;
  for (let c = 0; c < 3; c++) {
    buffer[index + c] = buffer[index + c]! + (color[c]! - buffer[index + c]!) * amount;
  }
}

function addRing(buffer: Float32Array, cx: number, cy: number, color: number[]): void {
  for (let y = 0; y < GRID; y++) {
    for (let x = 0; x < GRID; x++) {
      const dx = x + 0.5 - cx;
      const dy = y + 0.5 - cy;
      const amount = Math.min(Math.max(1 - Math.hypot(dx, dy) / 0.9, 0), 1);
      if (amount > 0) blend(buffer, x, y, color, amount);
    }
  }
}

function addArc(
  buffer: Float32Array,
  radius: number,
  center: number,
  width: number,
  thickness: number,
  color: number[],
  intensity: number,
): void {
  for (let y = 0; y < GRID; y++) {
    for (let x = 0; x < GRID; x++) {
      const dx = x + 0.5 - 16;
      const dy = y + 0.5 - 16;
      const distance = Math.hypot(dx, dy);
      const radial = 1 - Math.abs(distance - radius) / thickness;
      const angular = 1 - Math.abs(wrapAngle(Math.atan2(dy, dx) - center)) / (width * 0.5);
      const amount = Math.min(Math.max(Math.min(radial, angular) * intensity, 0), 1);
      if (amount > 0) blend(buffer, x, y, color, amount);
    }
  }
}

function spawnPulses(state: CoverState, song: SongModel, order: number, row: number): void {
  for (let ch = 0; ch < Math.min(song.channels.length, 4); ch++) {
    const channel = song.channels[ch]!;
    const patternIndex = channel.orderList[order];
    const cell =
      patternIndex === undefined ? undefined : channel.patterns.get(patternIndex)?.rows[row];
    const note = cell?.note;
    if (!note || (note.kind !== "note" && note.kind !== "rawFreq")) continue;
    const instrument = channel.insTimeline[order]?.[row] ?? null;
    const info = instrument !== null ? song.instruments[instrument] : undefined;
    const color: [number, number, number] = info
      ? [info.colorRgb[0], info.colorRgb[1], info.colorRgb[2]]
      : [201, 151, 58];
    const volume = Math.min(Math.max((cell.volume ?? 15) / 15, 0.15), 1);
    state.overall = Math.max(state.overall, volume);
    const seed = ((order * 0x9e3779b9) ^ (row * 0x85ebca6b) ^ ch) >>> 0;
    const jitter = (randomWithSeed(seed) * 2 - 1) * 0.3;
    state.pulses.push({
      angle: (CHANNEL_ANGLES[ch] ?? 0) + jitter,
      age: 0,
      maxAge: 0.32 + volume * 0.28,
      color,
      width: 0.55 + volume * 0.55,
    });
  }
}

function step(state: CoverState, song: SongModel, backend: AudioBackend, dt: number): void {
  state.time += dt;
  state.overall = Math.max(0, state.overall - dt * 0.7);
  const playing = backend.isPlaying();
  const pos = songPositionAt(song, backend.currentTime());
  const key = playing ? `${pos.orderPos}:${pos.row}` : null;
  if (playing && key && key !== state.lastPosition) {
    spawnPulses(state, song, pos.orderPos, pos.row);
  }
  state.lastPosition = key;
  for (const pulse of state.pulses) pulse.age += dt;
  state.pulses = state.pulses.filter((p) => p.age < p.maxAge);
}

function render(state: CoverState, data: Uint8ClampedArray): void {
  const buffer = new Float32Array(GRID * GRID * 3);

  for (let cy = 0; cy < 16; cy++) {
    for (let cx = 0; cx < 16; cx++) {
      const index = cy * 16 + cx;
      const wave = 0.5 + 0.5 * Math.sin(state.time * state.speeds[index]! * TAU + state.phases[index]!);
      const spike = wave ** 4;
      const color = [3 + 42 * spike, 9 + 186 * spike, 16 + 216 * spike];
      for (let dy = 0; dy < 2; dy++) {
        for (let dx = 0; dx < 2; dx++) {
          blend(buffer, cx * 2 + dx, cy * 2 + dy, color, 1);
        }
      }
    }
  }

  const spin = state.time * 0.25 * TAU;
  const boost = 0.85 + state.overall * 0.3;
  for (let y = 0; y < GRID; y++) {
    for (let x = 0; x < GRID; x++) {
      const dx = x + 0.5 - 16;
      const dy = y + 0.5 - 16;
      const distance = Math.hypot(dx, dy);
      if (distance > DISC_R + 0.5) continue;
      const edge = distance > 12.5 ? Math.max(13.5 - distance, 0) : 1;
      let color: number[];
      if (distance <= 2.3) {
        color = [10, 12, 20];
      } else if (distance <= 3.2) {
        color = [178, 186, 206];
      } else {
        color = [138, 150, 172];
        const angle = Math.atan2(dy, dx);
        const specs: Array<[number, number, number]> = [
          [spin, boost, 0.35],
          [spin + Math.PI, 0.32 * boost, 0.28],
        ];
        for (const [center, scale, sigma] of specs) {
          const delta = wrapAngle(angle - center);
          const highlight = Math.exp(-(delta * delta) / (2 * sigma * sigma)) * scale;
          color = color.map((c, i) => c + ([230, 240, 255][i]! - c) * highlight);
        }
        if (distance > DISC_R - 1.2) {
          color = color.map((c, i) => c + ([70, 78, 98][i]! - c) * 0.4);
        }
      }
      const index = (y * GRID + x) * 3;
      for (let c = 0; c < 3; c++) {
        buffer[index + c] = buffer[index + c]! + (color[c]! - buffer[index + c]!) * edge;
      }
    }
  }

  addRing(
    buffer,
    16 + Math.cos(spin) * 11.5,
    16 + Math.sin(spin) * 11.5,
    [255, 255, 255],
  );

  for (const pulse of state.pulses) {
    const t = pulse.age / pulse.maxAge;
    const envelope = t < 0.2 ? t / 0.2 : 1 - (t - 0.2) / 0.8;
    addArc(buffer, DISC_R + 1.3, pulse.angle, pulse.width, 1.6, pulse.color, envelope * 1.5);
  }

  for (let y = 0; y < GRID; y++) {
    for (let x = 0; x < GRID; x++) {
      const index = (y * GRID + x) * 3;
      const p = (y * GRID + x) * 4;
      data[p] = dither(buffer[index]!, x, y);
      data[p + 1] = dither(buffer[index + 1]!, x, y);
      data[p + 2] = dither(buffer[index + 2]!, x, y);
      data[p + 3] = 255;
    }
  }
}

interface CoverArtProps {
  song: SongModel;
  backend: AudioBackend;
  title: string;
}

export function CoverArt({ song, backend, title }: CoverArtProps) {
  const explain = useExplainer();
  const displayRef = useRef<HTMLCanvasElement | null>(null);
  const sourceRef = useRef<HTMLCanvasElement | null>(null);
  const stateRef = useRef<CoverState>({
    time: 0,
    last: 0,
    overall: 0,
    phases: new Float32Array(256),
    speeds: new Float32Array(256),
    pulses: [],
    lastPosition: null,
  });

  useEffect(() => {
    const random = makeRandom();
    const state = stateRef.current;
    for (let i = 0; i < 256; i++) {
      state.phases[i] = random() * TAU;
      state.speeds[i] = 0.12 + random() * 0.3;
    }
    const source = document.createElement("canvas");
    source.width = GRID;
    source.height = GRID;
    sourceRef.current = source;
    const ctx = source.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const loop = (now: number) => {
      const t = now / 1000;
      const dt = Math.min(Math.max(t - (state.last || t), 0), 0.1);
      state.last = t;
      step(state, song, backend, dt);
      const image = ctx.createImageData(GRID, GRID);
      render(state, image.data);
      ctx.putImageData(image, 0, 0);
      const display = displayRef.current;
      if (display) {
        const dctx = display.getContext("2d");
        if (dctx) {
          dctx.imageSmoothingEnabled = false;
          dctx.clearRect(0, 0, display.width, display.height);
          dctx.drawImage(source, 0, 0, display.width, display.height);
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [song, backend]);

  const exportPng = async () => {
    const source = sourceRef.current;
    if (!source) return;
    const size = 1600;
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(source, 0, 0, size, size);
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob((b) => resolve(b), "image/png"),
    );
    if (!blob) return;
    const bytes = new Uint8Array(await blob.arrayBuffer());
    await saveFile(`${safeFilename(title)}-cover.png`, bytes);
  };

  return (
    <section className="panel" onMouseEnter={() => explain(coverArtExplain())}>
      <h2>COVER ART</h2>
      <canvas
        ref={displayRef}
        className="cover-canvas"
        width={240}
        height={240}
        onClick={() => void exportPng()}
        title="Click to save a crisp 1600×1600 PNG"
      />
      <p className="hint">Click to save a crisp 1600×1600 PNG</p>
    </section>
  );
}
