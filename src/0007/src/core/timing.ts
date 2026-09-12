import type { SongMeta, SongModel } from "./songModel";

/** Seconds per pattern row (only speedPattern[0] is used, as in the original). */
export function rowDurationSec(meta: SongMeta): number {
  const speed = meta.speedPattern[0] ?? 6;
  return speed / Math.max(meta.tickRate, 1);
}

export interface RowTiming {
  starts: number[];
  ticks: number[];
}

/** Build one loop's row clock. Timing effects take effect on their own row. */
export function buildRowTiming(song: SongModel): RowTiming {
  const rowCount = song.meta.orderLength * song.meta.patternLength;
  const starts: number[] = [];
  const ticks: number[] = [];
  let time = 0;
  let tickRate = Math.max(song.meta.tickRate, 1);
  const speeds = song.meta.speedPattern.slice();
  if (speeds.length === 0) speeds.push(6);
  let virtualNum = Math.max(song.meta.virtualTempo[0], 1);
  let virtualDen = Math.max(song.meta.virtualTempo[1], 1);

  for (let absoluteRow = 0; absoluteRow < rowCount; absoluteRow++) {
    starts.push(time);
    const order = Math.floor(absoluteRow / song.meta.patternLength);
    const row = absoluteRow % song.meta.patternLength;
    for (const channel of song.channels) {
      const patternIndex = channel.orderList[order];
      if (patternIndex === undefined) continue;
      const pattern = channel.patterns.get(patternIndex);
      const cell = pattern?.rows[row];
      if (!cell) continue;
      const effectCount = channel.effectColumns;
      for (let e = 0; e < effectCount && e < cell.effects.length; e++) {
        const effect = cell.effects[e]!;
        const command = effect.effect;
        const value = effect.value;
        if (command === null || value === null) continue;
        if (command === 0x09 && value > 0) {
          speeds[0] = value;
        } else if (command === 0x0f && value > 0) {
          if (speeds.length < 2) speeds.push(value);
          else speeds[1] = value;
        } else if (command === 0xf0 && value > 0) {
          tickRate = Math.max((value * 2) / 5, 1);
        } else if (command >= 0xc0 && command <= 0xc3) {
          const hz = (((command & 3) << 8) | value) & 0xffff;
          if (hz > 0) tickRate = hz;
        } else if (command === 0xfd && value > 0) {
          virtualNum = value;
        } else if (command === 0xfe && value > 0) {
          virtualDen = value;
        }
      }
    }
    const speed = Math.max(speeds[absoluteRow % speeds.length] ?? 1, 1);
    ticks.push(speed);
    time += (speed / tickRate) * (virtualDen / virtualNum);
  }
  starts.push(time);
  return { starts, ticks };
}

export interface SongPosition {
  orderPos: number;
  row: number;
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

/** Partition-point over `items[0..end)` without allocating a slice. */
function partitionPointRange<T>(
  items: ArrayLike<T>,
  end: number,
  predicate: (item: T) => boolean,
): number {
  let lo = 0;
  let hi = Math.min(end, items.length);
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (predicate(items[mid] as T)) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Absolute song-time (seconds) -> (order position, row). Clamps negatives. */
export function songPositionAt(song: SongModel, t: number): SongPosition {
  const rows = Math.max(song.rowTimes.length - 1, 1);
  const duration = song.rowTimes[song.rowTimes.length - 1] ?? 0;
  let time = 0;
  if (duration > 0) {
    time = (((t < 0 ? 0 : t) % duration) + duration) % duration;
  }
  const totalRows = Math.max(
    partitionPointRange(song.rowTimes, rows, (start) => start <= time) - 1,
    0,
  );
  const patternLength = Math.max(song.meta.patternLength, 1);
  return {
    orderPos: Math.floor(totalRows / patternLength) % Math.max(song.meta.orderLength, 1),
    row: totalRows % patternLength,
  };
}

export function rowTime(song: SongModel, order: number, row: number): number {
  const index = order * song.meta.patternLength + row;
  return song.rowTimes[index] ?? 0;
}

export function rowDuration(song: SongModel, absoluteRow: number): number {
  const start = song.rowTimes[absoluteRow];
  const end = song.rowTimes[absoluteRow + 1];
  if (start !== undefined && end !== undefined) return end - start;
  return rowDurationSec(song.meta);
}
