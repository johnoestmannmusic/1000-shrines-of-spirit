import { memo, useState } from "react";
import type { SongModel } from "@/core/songModel";
import type { ProjectFile } from "@/core/project";
import type { AudioBackend, SamplePlayhead } from "@/audio/backend";
import { useAnimationFrame } from "../hooks";
import { useExplainer } from "../explainer";
import {
  clearSamplesExplain,
  packageSamplesExplain,
  sampleInfoExplain,
  sampleLoadExplain,
  samplePreviewExplain,
  sourceSamplesExplain,
} from "../explainerContent";
import { Waveform, type WaveformMarker } from "./Waveform";

interface SourceSamplesProps {
  backend: AudioBackend;
  song: SongModel;
  project: ProjectFile | null;
  sampleNames: string[];
  onLoad: (slot: number) => void;
  onPlay: (slot: number) => void;
  onStop: () => void;
  onInfo: (slot: number) => void;
  onClear: () => void;
  onPackage: () => void;
}

function SourceSamplesImpl(props: SourceSamplesProps) {
  const explain = useExplainer();
  const [durations, setDurations] = useState<number[]>([]);
  const [playheads, setPlayheads] = useState<SamplePlayhead[]>([]);

  useAnimationFrame(() => {
    const nextDurations = props.backend.sampleDurations();
    setDurations((prev) =>
      prev.length === nextDurations.length && prev.every((v, i) => v === nextDurations[i])
        ? prev
        : nextDurations,
    );
    setPlayheads(props.backend.samplePlayheads());
  }, 12);

  const populated = durations.filter((d) => d > 0).length;

  return (
    <section className="panel">
      <div className="row">
        <h2 style={{ margin: 0, cursor: "help" }} onMouseEnter={() => explain(sourceSamplesExplain())}>SOURCE SAMPLES</h2>
        <span className="spacer" />
        <span className="muted small">{populated} / 6</span>
        <button onClick={props.onPackage} onMouseEnter={() => explain(packageSamplesExplain())}>Package Samples</button>
        <button onClick={props.onClear} onMouseEnter={() => explain(clearSamplesExplain())}>Clear Samples</button>
      </div>
      <p className="hint">
        Six fixed slots · package session imports as numbered WAV files for hosting.
      </p>
      <div className="sample-slots">
        {Array.from({ length: 6 }, (_, slot) => {
          const duration = durations[slot] ?? 0;
          const name = props.sampleNames[slot] ?? "";
          const peaks = props.backend.sampleWaveform(slot);
          const markers: WaveformMarker[] = playheads
            .filter((p) => p.source === slot && !p.fused && duration > 0)
            .map((p) => {
              const color =
                p.instrument !== null
                  ? `rgb(${props.song.instruments[p.instrument]?.colorRgb.join(",") ?? "201,151,58"})`
                  : "rgb(201,151,58)";
              return {
                fraction: p.position / duration,
                color,
                label: p.instrument ?? undefined,
                alpha: p.level,
              };
            });
          return (
            <div className="sample-slot" key={slot}>
              <div className="row">
                <strong className="mono">Slot {slot}</strong>
                {duration > 0 ? (
                  <span className="muted small">
                    · {name || "untitled"} · {duration.toFixed(2)}s
                  </span>
                ) : (
                  <span className="muted small">— empty (assets/SourceSamples/{slot}.ogg)</span>
                )}
                <span className="spacer" />
                {duration > 0 && (
                  <>
                    <button onClick={() => props.onPlay(slot)} onMouseEnter={() => explain(samplePreviewExplain())}>▶</button>
                    <button onClick={props.onStop}>■</button>
                    <button onClick={() => props.onInfo(slot)} onMouseEnter={() => explain(sampleInfoExplain())}>Info</button>
                  </>
                )}
                <button onClick={() => props.onLoad(slot)} onMouseEnter={() => explain(sampleLoadExplain())}>Load</button>
              </div>
              {duration > 0 && (
                <Waveform peaks={peaks} markers={markers} height={48} emptyLabel="(decoding…)" />
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

export const SourceSamples = memo(SourceSamplesImpl);
