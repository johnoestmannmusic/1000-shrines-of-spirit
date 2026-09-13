import { useRef, useState, memo } from "react";
import type { SongModel } from "@/core/songModel";
import { noteToName, A_REF_NOTE } from "@/core/pitch";
import { songPositionAt } from "@/core/timing";
import type { AudioBackend } from "@/audio/backend";
import { useAnimationFrame } from "../hooks";
import { useExplainer } from "../explainer";
import { pianoRollExplain } from "../explainerContent";

const BLACK = new Set([1, 3, 6, 8, 10]);
const WHITE_WIDTH = 22;
const WHITE_HEIGHT = 60;
const BLACK_WIDTH = 13;
const BLACK_HEIGHT = 38;

interface PianoProps {
  song: SongModel;
  backend: AudioBackend;
}

function PianoImpl({ song, backend }: PianoProps) {
  const explain = useExplainer();
  const [, force] = useState(0);
  const lastPosition = useRef<string | null>(null);
  const flashes = useRef<Array<{ note: number; color: string; until: number }>>([]);
  const noise = useRef<{ name: string; color: string; until: number } | null>(null);

  useAnimationFrame(() => {
    const now = performance.now() / 1000;
    const pos = songPositionAt(song, backend.currentTime());
    const key = `${pos.orderPos}:${pos.row}`;
    const playing = backend.isPlaying();
    if (playing && key !== lastPosition.current) {
      spawnFlashes(song, pos.orderPos, pos.row, now);
    }
    lastPosition.current = playing ? key : null;
    const activeFlash = flashes.current.some((f) => f.until > now);
    const activeNoise = noise.current !== null && noise.current.until > now;
    if (!playing && !activeFlash && !activeNoise) return;
    force((n) => n + 1);
  }, 12);

  const spawnFlashes = (model: SongModel, order: number, row: number, now: number) => {
    for (let ch = 0; ch < Math.min(model.channels.length, 4); ch++) {
      const channel = model.channels[ch]!;
      const patternIndex = channel.orderList[order];
      const cell = patternIndex === undefined ? undefined : channel.patterns.get(patternIndex)?.rows[row];
      const note = cell?.note;
      if (!note || (note.kind !== "note" && note.kind !== "rawFreq")) continue;
      const instrument = channel.insTimeline[order]?.[row] ?? null;
      const info = instrument !== null ? model.instruments[instrument] : undefined;
      const color = info ? `rgb(${info.colorRgb.join(",")})` : "rgb(201,151,58)";
      if (ch === 3) {
        noise.current = {
          name: note.kind === "note" ? noteToName(note) : "FRQ",
          color,
          until: now + 0.3,
        };
      } else if (note.kind === "note") {
        flashes.current = flashes.current.filter((f) => f.note !== note.note);
        flashes.current.push({ note: note.note, color, until: now + 0.18 });
      }
    }
  };

  const now = performance.now() / 1000;
  flashes.current = flashes.current.filter((f) => f.until > now);
  const pos = songPositionAt(song, backend.currentTime());

  const range = noteRange(song);
  const notes: number[] = [];
  for (let n = range.min; n <= range.max; n++) notes.push(n);

  const heldTonal = new Map<number, string>();
  for (let ch = 0; ch < Math.min(song.channels.length, 3); ch++) {
    const channel = song.channels[ch]!;
    const note = channel.noteTimeline[pos.orderPos]?.[pos.row];
    const instrument = channel.insTimeline[pos.orderPos]?.[pos.row] ?? null;
    if (note && note.kind === "note" && instrument !== null) {
      const info = song.instruments[instrument];
      heldTonal.set(note.note, info ? `rgb(${info.colorRgb.join(",")})` : "#c9973a");
    }
  }

  let whiteIndex = 0;
  const whiteKeys: Array<{ note: number; x: number }> = [];
  const blackKeys: Array<{ note: number; x: number }> = [];
  for (const note of notes) {
    if (!BLACK.has(note % 12)) {
      whiteKeys.push({ note, x: whiteIndex * WHITE_WIDTH });
      whiteIndex += 1;
    } else {
      blackKeys.push({ note, x: whiteIndex * WHITE_WIDTH - BLACK_WIDTH / 2 });
    }
  }
  const width = whiteIndex * WHITE_WIDTH + 1;

  const activeColor = (note: number): string => {
    const flash = [...flashes.current].reverse().find((f) => f.note === note);
    if (flash) return flash.color;
    return heldTonal.get(note) ?? (BLACK.has(note % 12) ? "#0c0c0e" : "#323236");
  };

  const sustainedNoise = heldNoise(song, pos.orderPos, pos.row);

  return (
    <section className="panel" onMouseEnter={() => explain(pianoRollExplain())}>
      <h2>PIANO</h2>
      <div className="piano-row">
        <div className="piano-scroll">
          <svg width={width} height={WHITE_HEIGHT + 1}>
            {whiteKeys.map(({ note, x }) => (
              <g key={`w${note}`}>
                <rect
                  x={x}
                  y={0}
                  width={WHITE_WIDTH}
                  height={WHITE_HEIGHT}
                  fill={activeColor(note)}
                  stroke="#8a8a8a"
                  strokeWidth={1}
                />
                {note % 12 === 0 && (
                  <text x={x + 3} y={WHITE_HEIGHT - 4} fontSize={9} fill="#b9b9b9">
                    {noteToName({ kind: "note", note })}
                  </text>
                )}
              </g>
            ))}
            {blackKeys.map(({ note, x }) => (
              <rect
                key={`b${note}`}
                x={x}
                y={0}
                width={BLACK_WIDTH}
                height={BLACK_HEIGHT}
                fill={activeColor(note)}
                stroke="#000"
                strokeWidth={1}
              />
            ))}
          </svg>
        </div>
        <div className="noise-box">
          <div className="muted small">NOISE</div>
          {noise.current && noise.current.until > now ? (
            <span className="mono noise-note" style={{ color: noise.current.color }}>
              {noise.current.name}
            </span>
          ) : sustainedNoise ? (
            <span className="mono noise-note" style={{ color: sustainedNoise.color }}>
              {sustainedNoise.name}
            </span>
          ) : (
            <span className="muted">—</span>
          )}
        </div>
      </div>
    </section>
  );
}

function noteRange(song: SongModel): { min: number; max: number } {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (let ch = 0; ch < Math.min(song.channels.length, 3); ch++) {
    for (const order of song.channels[ch]!.noteTimeline) {
      for (const note of order) {
        if (note && note.kind === "note") {
          min = Math.min(min, note.note);
          max = Math.max(max, note.note);
        }
      }
    }
  }
  if (!Number.isFinite(min)) {
    min = A_REF_NOTE - 12;
    max = A_REF_NOTE + 12;
  }
  return { min: Math.max(min - 2, 0), max: Math.min(max + 2, 179) };
}

function heldNoise(
  song: SongModel,
  order: number,
  row: number,
): { name: string; color: string } | null {
  const channel = song.channels[3];
  if (!channel) return null;
  const note = channel.noteTimeline[order]?.[row];
  if (!note || note.kind !== "note") return null;
  const instrument = channel.insTimeline[order]?.[row] ?? null;
  const info = instrument !== null ? song.instruments[instrument] : undefined;
  return {
    name: noteToName(note),
    color: info ? `rgb(${info.colorRgb.join(",")})` : "#c9973a",
  };
}

export const Piano = memo(PianoImpl);
