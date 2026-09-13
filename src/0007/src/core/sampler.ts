import type { NoteValue } from "./fur/types";
import { samplerPlaybackRate } from "./pitch";
import type { SongModel } from "./songModel";
import { defaultSpectralSettings, type SpectralSettings } from "./spectral";
import { rowDuration } from "./timing";

export const LOOKAHEAD_SEC = 0.15;
export const POLL_INTERVAL_MS = 25;

export interface SamplerSettings {
  sourceIndex: number | null;
  startSec: number;
  endSec: number;
  transpose: number;
  volume: number;
  /** Serialised in Project JSON as `loop`. */
  looping: boolean;
  pingPong: boolean;
  attack: number;
  decay: number;
  sustain: number;
  release: number;
  /** Stereo pan centre, -1 (left) .. 1 (right). */
  pan: number;
  /** Random pan width around the centre, 0..1. */
  panRandomRange: number;
  /** Vibrato LFO speed in Hz. */
  vibratoSpeed: number;
  /** Vibrato depth in semitones (0 = off). */
  vibratoDepth: number;
  polyphonic: boolean;
  voiceCap: number;
  spectral: SpectralSettings;
  /** Not persisted (matches the Rust `#[serde(skip)]`). */
  muted: boolean;
}

export function defaultSamplerSettings(): SamplerSettings {
  return {
    sourceIndex: null,
    startSec: 0,
    endSec: 0,
    transpose: 0,
    volume: 1,
    looping: false,
    pingPong: false,
    attack: 0.005,
    decay: 0.08,
    sustain: 0.7,
    release: 0.15,
    pan: 0,
    panRandomRange: 0,
    vibratoSpeed: 6,
    vibratoDepth: 0,
    polyphonic: false,
    voiceCap: 8,
    spectral: defaultSpectralSettings(),
    muted: false,
  };
}

export function setSpectralEnabled(settings: SamplerSettings, enabled: boolean): void {
  if (enabled === settings.spectral.enabled) return;
  if (enabled) {
    settings.spectral.savedStartSec = settings.startSec;
    settings.spectral.savedEndSec = settings.endSec;
    settings.spectral.savedLooping = settings.looping;
    // Spectral renders are sustained loops by default.
    settings.looping = true;
  } else {
    settings.startSec = settings.spectral.savedStartSec;
    settings.endSec = settings.spectral.savedEndSec;
    settings.looping = settings.spectral.savedLooping;
  }
  settings.spectral.enabled = enabled;
}

export function envelopeAt(settings: SamplerSettings, time: number): number {
  const attack = Math.min(Math.max(settings.attack, 0.003), 5);
  const decay = Math.min(Math.max(settings.decay, 0), 5);
  if (time <= 0) return 0;
  if (time < attack) return time / attack;
  if (time < attack + decay) return 1 + (clamp01(settings.sustain) - 1) * ((time - attack) / decay);
  return clamp01(settings.sustain);
}

function clamp01(v: number): number {
  return Math.min(Math.max(v, 0), 1);
}

export function samplePosition(settings: SamplerSettings, elapsed: number, rate: number): number {
  const length = settings.endSec - settings.startSec;
  if (length <= 0) return settings.startSec;
  const traveled = Math.max(elapsed, 0) * rate;
  let position: number;
  if (settings.looping && settings.pingPong) {
    const phase = ((traveled % (2 * length)) + 2 * length) % (2 * length);
    position = phase <= length ? phase : 2 * length - phase;
  } else if (settings.looping) {
    position = ((traveled % length) + length) % length;
  } else {
    position = Math.min(traveled, length);
  }
  return settings.startSec + position;
}

/** Returns [start, length] for a valid slice of decoded audio, else null. */
export function region(settings: SamplerSettings, duration: number): [number, number] | null {
  if (
    !Number.isFinite(duration) ||
    duration <= 0 ||
    !Number.isFinite(settings.startSec) ||
    !Number.isFinite(settings.endSec)
  ) {
    return null;
  }
  const start = Math.min(Math.max(settings.startSec, 0), duration);
  const end = Math.min(Math.max(settings.endSec, 0), duration);
  return end > start ? [start, end - start] : null;
}

export type SamplerEvent =
  | { type: "note"; channel: number; instrument: number; rate: number; volume: number }
  | { type: "off"; channel: number }
  | { type: "pitchRamp"; channel: number; rate: number; duration: number };

export interface Sequence {
  tuning: number;
  rows: SamplerEvent[][];
  /** Absolute row starts followed by the loop end. */
  rowTimes: number[];
}

export function sequenceDuration(sequence: Sequence): number {
  return sequence.rowTimes[sequence.rowTimes.length - 1] ?? 0;
}

export function sequenceFromSong(song: SongModel): Sequence {
  const rows: SamplerEvent[][] = [];
  const baseRates: (number | null)[] = [null, null, null, null];
  const pitchOffsets = [0, 0, 0, 0];
  const slides = [0, 0, 0, 0];

  for (let order = 0; order < song.meta.orderLength; order++) {
    for (let row = 0; row < song.meta.patternLength; row++) {
      const events: SamplerEvent[] = [];
      const absoluteRow = order * song.meta.patternLength + row;
      const channelCount = Math.min(song.channels.length, 4);
      for (let channel = 0; channel < channelCount; channel++) {
        const ch = song.channels[channel]!;
        const patternIndex = ch.orderList[order];
        const pattern = patternIndex === undefined ? undefined : ch.patterns.get(patternIndex);
        const cell = pattern?.rows[row];
        if (!cell) continue;

        const note: NoteValue | null = cell.note;
        if (note) {
          if (note.kind === "off" || note.kind === "release") {
            events.push({ type: "off", channel });
            baseRates[channel] = null;
          } else if (note.kind === "note" || note.kind === "rawFreq") {
            const instrument = ch.insTimeline[order]?.[row] ?? null;
            const rate = samplerPlaybackRate(note, song.meta.tuningA4, 0);
            if (
              instrument !== null &&
              rate !== null &&
              instrument < song.instruments.length &&
              Number.isFinite(rate) &&
              rate > 0
            ) {
              events.push({
                type: "note",
                channel,
                instrument,
                rate,
                volume: Math.min(cell.volume ?? 15, 15) / 15,
              });
              baseRates[channel] = rate;
              pitchOffsets[channel] = 0;
            }
          }
        }

        const effectCount = Math.min(ch.effectColumns, cell.effects.length);
        for (let e = 0; e < effectCount; e++) {
          const effect = cell.effects[e]!;
          if (effect.effect === 0x01 && effect.value !== null) {
            slides[channel] = effect.value / 32;
          } else if (effect.effect === 0x02 && effect.value !== null) {
            slides[channel] = -effect.value / 32;
          }
        }

        if (slides[channel] !== 0) {
          const baseRate = baseRates[channel];
          if (baseRate !== null) {
            const ticks = song.rowTicks[absoluteRow] ?? 6;
            pitchOffsets[channel] = (pitchOffsets[channel] ?? 0) + (slides[channel] ?? 0) * ticks;
            events.push({
              type: "pitchRamp",
              channel,
              rate: baseRate * Math.pow(2, (pitchOffsets[channel] ?? 0) / 12),
              duration: rowDuration(song, absoluteRow),
            });
          }
        }
      }
      rows.push(events);
    }
  }

  return { tuning: song.meta.tuningA4, rows, rowTimes: song.rowTimes.slice() };
}

export interface ScheduledRow {
  row: number;
  when: number;
}

export class Scheduler {
  private readonly rowTimes: number[];
  private readonly duration: number;
  private nextRow: number;
  private readonly startClock: number;
  private readonly offset: number;

  constructor(sequence: Sequence, startClock: number, offset: number) {
    const duration = sequenceDuration(sequence);
    if (!(Number.isFinite(duration) && duration > 0 && sequence.rows.length > 0)) {
      throw new Error("Scheduler requires a positive-duration, non-empty sequence");
    }
    const clampedOffset = Math.max(offset, 0);
    this.rowTimes = sequence.rowTimes.slice();
    this.duration = duration;
    this.startClock = startClock;
    this.offset = clampedOffset;
    const cycle = Math.floor(clampedOffset / duration);
    const within = ((clampedOffset % duration) + duration) % duration;
    const row = Math.max(
      partitionPoint(this.rowTimes.slice(0, sequence.rows.length), (time) => time <= within) - 1,
      0,
    );
    this.nextRow = cycle * sequence.rows.length + row;
  }

  tick(now: number, lookahead: number): ScheduledRow[] {
    const songTime = this.offset + now - this.startClock;
    const rowCount = this.rowTimes.length - 1;
    const locate = (time: number): number => {
      const cycle = Math.max(Math.floor(time / this.duration), 0);
      const within = ((time % this.duration) + this.duration) % this.duration;
      const row = Math.max(
        partitionPoint(this.rowTimes.slice(0, rowCount), (start) => start <= within) - 1,
        0,
      );
      return cycle * rowCount + row;
    };
    this.nextRow = Math.max(this.nextRow, locate(Math.max(songTime, this.offset)));
    const horizon = songTime + Math.max(lookahead, 0);
    const result: ScheduledRow[] = [];
    for (;;) {
      const cycle = Math.floor(this.nextRow / rowCount);
      const row = this.nextRow % rowCount;
      const time = cycle * this.duration + (this.rowTimes[row] ?? 0);
      if (time > horizon) break;
      result.push({
        row,
        when: Math.max(
          Math.max(this.startClock + time - this.offset, now),
          this.startClock,
        ),
      });
      this.nextRow += 1;
    }
    return result;
  }
}

function partitionPoint<T>(items: ArrayLike<T>, predicate: (item: T) => boolean): number {
  let lo = 0;
  let hi = items.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (predicate(items[mid] as T)) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** One channel of an isolated loop, with 5 ms edge fades and optional reverse half. */
export function loopChannel(
  data: Float32Array,
  sampleRate: number,
  start: number,
  end: number,
  pingPong: boolean,
): Float32Array {
  const first = Math.min(Math.floor(Math.max(start, 0) * sampleRate), data.length);
  const last = Math.min(Math.ceil(Math.max(end, 0) * sampleRate), data.length);
  if (first >= last) return new Float32Array(0);
  const src = data.subarray(first, last);
  const len = src.length;
  const fade = Math.min(Math.round(0.005 * sampleRate), Math.floor(len / 2));
  const regionOut = new Float32Array(len);
  regionOut.set(src);
  for (let i = 0; i < fade; i++) {
    const gain = i / fade;
    regionOut[i] = regionOut[i]! * gain;
    regionOut[len - 1 - i] = regionOut[len - 1 - i]! * gain;
  }
  if (!pingPong) return regionOut;
  const out = new Float32Array(len * 2);
  out.set(regionOut, 0);
  for (let i = 0; i < len; i++) out[len + i] = regionOut[len - 1 - i]!;
  return out;
}

/** Min/max pairs for waveform display. */
export function waveform(data: Float32Array, bins: number): Array<[number, number]> {
  if (data.length === 0 || bins === 0) return [];
  const chunkSize = Math.ceil(data.length / bins);
  const out: Array<[number, number]> = [];
  for (let start = 0; start < data.length; start += chunkSize) {
    const end = Math.min(start + chunkSize, data.length);
    let low = 0;
    let high = 0;
    for (let i = start; i < end; i++) {
      const v = data[i]!;
      if (v < low) low = v;
      if (v > high) high = v;
    }
    out.push([low, high]);
  }
  return out;
}
