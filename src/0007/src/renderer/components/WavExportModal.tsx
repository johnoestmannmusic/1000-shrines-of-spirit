import { useState } from "react";
import { DraggableModal } from "./DraggableModal";
import { NumberInput } from "./NumberInput";
import type { ExportParams } from "@/core/export";

interface WavExportModalProps {
  busy: boolean;
  progress: number;
  onExport: (params: ExportParams) => void;
  onClose: () => void;
}

export function WavExportModal({ busy, progress, onExport, onClose }: WavExportModalProps) {
  const [loops, setLoops] = useState(0);
  const [fadeInMs, setFadeInMs] = useState(0);
  const [fadeOutMs, setFadeOutMs] = useState(0);
  const [normalize, setNormalize] = useState(true);

  if (busy) {
    const percent = Math.round(Math.min(Math.max(progress, 0), 1) * 100);
    return (
      <DraggableModal title="Export WAV" onClose={() => undefined} width={420}>
        <p className="hint">Exporting WAV… {percent}%</p>
        <div className="progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
          <div className="progress-fill" style={{ width: `${percent}%` }} />
        </div>
      </DraggableModal>
    );
  }

  return (
    <DraggableModal title="Export WAV" onClose={onClose} width={420}>
      <p className="hint">
        The current Master FX are applied to the exported file. Loops repeats the song after the
        first pass; a fade-out is an extra tail appended after the full loops that keeps looping
        while it ramps to silence. Peak normalisation sets the loudest peak to full scale.
      </p>

      <label className="slider">
        Loops
        <NumberInput
          value={loops}
          min={0}
          max={99}
          step={1}
          onChange={(v) => setLoops(Math.round(v))}
          ariaLabel="Number of Loops"
        />
        <span className="muted small">0 = play once</span>
      </label>
      <label className="slider">
        Fade In (ms)
        <NumberInput
          value={fadeInMs}
          min={0}
          max={60_000}
          step={10}
          onChange={setFadeInMs}
          ariaLabel="Fade In (ms)"
        />
      </label>
      <label className="slider">
        Fade Out (ms)
        <NumberInput
          value={fadeOutMs}
          min={0}
          max={60_000}
          step={10}
          onChange={setFadeOutMs}
          ariaLabel="Fade Out (ms)"
        />
      </label>

      <div className="row" style={{ marginTop: 8 }}>
        <label>
          <input
            type="checkbox"
            checked={normalize}
            onChange={(e) => setNormalize(e.target.checked)}
          />
          Peak Normalize
        </label>
      </div>

      <div className="row" style={{ marginTop: 12 }}>
        <button onClick={() => onExport({ loops, fadeInMs, fadeOutMs, normalize })}>Export WAV</button>
        <button onClick={onClose}>Cancel</button>
      </div>
    </DraggableModal>
  );
}
