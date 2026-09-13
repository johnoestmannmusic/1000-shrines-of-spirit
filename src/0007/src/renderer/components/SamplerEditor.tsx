import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { SamplerSettings } from "@/core/sampler";
import {
  SPECTRAL_FUSION_MODES,
  spectralModeHasAmount,
  spectralModeLabel,
  spectralModeNeedsB,
  MAX_LOOP_SECONDS,
  MIN_LOOP_SECONDS,
} from "@/core/spectral";
import type { AudioBackend } from "@/audio/backend";
import { useAnimationFrame } from "../hooks";
import { Waveform, type WaveformMarker } from "./Waveform";
import { AdsrGraph } from "./AdsrGraph";
import { NumberInput } from "./NumberInput";
import { useExplainer } from "../explainer";
import { spectralFusionExplain } from "../explainerContent";
import { formatDb } from "../util";

interface SamplerEditorProps {
  backend: AudioBackend;
  index: number;
  name: string;
  color: [number, number, number];
  settings: SamplerSettings;
  sampleNames: string[];
  reference: boolean;
  onReferenceChange: (value: boolean) => void;
  spectralTab: boolean;
  wasmAvailable: boolean;
  onTabChange: (tab: boolean) => void;
  onUpdate: (patch: Partial<SamplerSettings>) => void;
  onClose: () => void;
}

export function SamplerEditor(props: SamplerEditorProps) {
  const { backend, settings } = props;
  const explain = useExplainer();
  const [position, setPosition] = useState(() => ({
    // Open high and roughly centred so it rarely needs dragging up.
    x: Math.max(16, (window.innerWidth - 680) / 2),
    y: 24,
  }));
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const [duration, setDuration] = useState(0);
  const [peaks, setPeaks] = useState<Array<[number, number]>>([]);
  const [markers, setMarkers] = useState<WaveformMarker[]>([]);
  const [previewing, setPreviewing] = useState(false);
  const [rendering, setRendering] = useState(false);

  useAnimationFrame(() => {
    const dur = backend.effectiveDuration(props.index);
    setDuration(dur);
    setPeaks(backend.effectiveWaveform(props.index));
    const color = `rgb(${props.color.join(",")})`;
    setMarkers(
      backend
        .samplePlayheads()
        .filter((p) => p.instrument === props.index && dur > 0)
        .map((p) => ({
          fraction: p.position / dur,
          color,
          label: props.index,
          alpha: p.level,
        })),
    );
    setPreviewing(backend.previewPosition()?.instrument === props.index);
    setRendering(backend.fusionRendering(props.index));
  });

  const title = props.spectralTab ? "SpectralPrism" : `Sampler — ${props.name}`;

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("button")) return;
    drag.current = { dx: event.clientX - position.x, dy: event.clientY - position.y };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    setPosition({
      x: Math.max(0, event.clientX - drag.current.dx),
      y: Math.max(0, event.clientY - drag.current.dy),
    });
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const playable = duration > 0 && settings.endSec > settings.startSec;

  return (
    <div className="floating-window" style={{ left: position.x, top: position.y }}>
      <div
        className="floating-title"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        <span>{title}</span>
        <button
          className="close-btn"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={props.onClose}
        >
          ✕
        </button>
      </div>
      <div className="floating-body">
        <div className="row wrap">
          <strong
            style={{ cursor: "help" }}
            onMouseEnter={() => props.spectralTab && explain(spectralFusionExplain())}
          >
            {props.spectralTab ? "SPECTRAL" : "SAMPLER"}
          </strong>
          <label>
            <input
              type="checkbox"
              checked={props.reference}
              onChange={(e) => props.onReferenceChange(e.target.checked)}
            />
            Ref Pitch
          </label>
          <button
            disabled={!playable}
            onClick={() =>
              previewing ? backend.stopPreview() : backend.preview(props.index, props.reference)
            }
          >
            {previewing ? "Stop Preview" : "Preview"}
          </button>
          <label>
            <input
              type="checkbox"
              checked={settings.muted}
              onChange={(e) => props.onUpdate({ muted: e.target.checked })}
            />
            Mute in song
          </label>
        </div>
        <div className="row tabs">
          <button className={!props.spectralTab ? "active" : ""} onClick={() => props.onTabChange(false)}>
            Sampler
          </button>
          <button className={props.spectralTab ? "active" : ""} onClick={() => props.onTabChange(true)}>
            Spectral
          </button>
        </div>

        {props.spectralTab ? (
          <SpectralTab {...props} duration={duration} peaks={peaks} markers={markers} rendering={rendering} />
        ) : (
          <div className="editor-tab">
            <Waveform
              peaks={peaks}
              markers={markers}
              height={110}
              trim={{ start: settings.startSec, end: settings.endSec, duration }}
              onTrimChange={(start, end) => props.onUpdate({ startSec: start, endSec: end })}
              emptyLabel="Assign a source sample"
            />
            <div className="row wrap">
              <NumberField label="Start (s)" value={settings.startSec} min={0} max={duration} onChange={(v) => props.onUpdate({ startSec: v })} />
              <NumberField label="End (s)" value={settings.endSec} min={settings.startSec} max={duration} onChange={(v) => props.onUpdate({ endSec: v })} />
              <button onClick={() => props.onUpdate({ startSec: 0, endSec: duration })}>Reset Start/End</button>
            </div>
            <div className="row wrap">
              <label>Source</label>
              <select
                value={settings.sourceIndex === null ? "" : String(settings.sourceIndex)}
                onChange={(e) => props.onUpdate({ sourceIndex: e.target.value === "" ? null : Number(e.target.value) })}
              >
                <option value="">Unassigned</option>
                {props.sampleNames.map((n, slot) =>
                  n ? (
                    <option key={slot} value={String(slot)}>
                      {slot}: {n}
                    </option>
                  ) : null,
                )}
              </select>
              <NumberField label="Transpose (st)" value={settings.transpose} min={-48} max={48} step={0.01} onChange={(v) => props.onUpdate({ transpose: v })} />
              <label>
                <input type="checkbox" checked={settings.looping} onChange={(e) => props.onUpdate({ looping: e.target.checked })} />
                Loop
              </label>
              <label>
                <input type="checkbox" checked={settings.pingPong} disabled={!settings.looping} onChange={(e) => props.onUpdate({ pingPong: e.target.checked })} />
                Ping-pong
              </label>
            </div>

            <EnvelopeControls settings={settings} onUpdate={props.onUpdate} />

            <div className="row wrap">
              <Slider label="Pan" value={settings.pan * 100} min={-100} max={100} step={1} onChange={(v) => props.onUpdate({ pan: v / 100 })} />
              <Slider label="Random Pan (%)" value={settings.panRandomRange * 100} min={0} max={100} step={1} onChange={(v) => props.onUpdate({ panRandomRange: v / 100 })} />
              <Slider label="Vibrato Speed (Hz)" value={settings.vibratoSpeed} min={0} max={20} step={0.1} onChange={(v) => props.onUpdate({ vibratoSpeed: v })} />
              <Slider label="Vibrato Depth (st)" value={settings.vibratoDepth} min={0} max={2} step={0.01} onChange={(v) => props.onUpdate({ vibratoDepth: v })} />
              <label>
                <input type="checkbox" checked={settings.polyphonic} onChange={(e) => props.onUpdate({ polyphonic: e.target.checked })} />
                Polyphonic
              </label>
              <label>
                Voice cap
                <NumberInput
                  value={settings.voiceCap}
                  min={1}
                  max={32}
                  step={1}
                  disabled={!settings.polyphonic}
                  onChange={(v) => props.onUpdate({ voiceCap: Math.round(v) })}
                />
              </label>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function SpectralTab(
  props: SamplerEditorProps & {
    duration: number;
    peaks: Array<[number, number]>;
    markers: WaveformMarker[];
    rendering: boolean;
  },
) {
  const { backend, settings } = props;
  const spectral = settings.spectral;
  const needsB = spectralModeNeedsB(spectral.mode);
  const fusionReady = backend.fusionReady(props.index);
  const color = `rgb(${props.color.join(",")})`;

  const patchSpectral = (patch: Partial<typeof spectral>) =>
    props.onUpdate({ spectral: { ...spectral, ...patch } });

  const sourceAWave =
    settings.sourceIndex === null ? [] : backend.sampleWaveform(settings.sourceIndex);
  const sourceBWave =
    spectral.sourceIndex2 === null ? [] : backend.sampleWaveform(spectral.sourceIndex2);
  const resultWave = backend.fusionWaveform(props.index);
  const resultMarkers: WaveformMarker[] = props.markers
    .filter((m) => m.fraction >= 0 && m.fraction <= 1)
    .map((m) => ({ ...m, color }));

  const amountKey =
    spectral.mode === "mix"
      ? "mixAmount"
      : spectral.mode === "cross-synth"
        ? "crossSynthAmount"
        : spectral.mode === "convolve"
          ? "convolveAmount"
          : "ringModAmount";
  const amount = spectral[amountKey];

  return (
    <div className="editor-tab">
      <p className="hint">
        {!props.wasmAvailable
          ? "The prism_dsp WASM engine failed to load — Spectral renders are disabled."
          : props.rendering
            ? "Rendering…"
            : fusionReady
              ? "Rendered result is ready."
              : "Choose Sample A (and B where required) — rendering is automatic."}
      </p>
      <div className="row wrap">
        <label>Mode</label>
        <select value={spectral.mode} onChange={(e) => patchSpectral({ mode: e.target.value as typeof spectral.mode })}>
          {SPECTRAL_FUSION_MODES.map((mode) => (
            <option key={mode} value={mode}>
              {spectralModeLabel(mode)}
            </option>
          ))}
        </select>
      </div>

      <h3>Sample A</h3>
      <div className="row wrap">
        <label>Source</label>
        <select
          value={settings.sourceIndex === null ? "" : String(settings.sourceIndex)}
          onChange={(e) =>
            props.onUpdate({ sourceIndex: e.target.value === "" ? null : Number(e.target.value) })
          }
        >
          <option value="">Unassigned</option>
          {props.sampleNames.map((n, slot) =>
            n ? (
              <option key={slot} value={String(slot)}>
                {slot}: {n}
              </option>
            ) : null,
          )}
        </select>
      </div>
      <Waveform
        peaks={sourceAWave}
        freezePoint={spectral.freezePoint}
        onFreezeChange={(v) => patchSpectral({ freezePoint: v })}
        height={56}
        emptyLabel="Assign a source sample"
      />
      <div className="row wrap">
        <Slider label="Freeze Point (%)" value={spectral.freezePoint} min={0} max={100} step={1} onChange={(v) => patchSpectral({ freezePoint: v })} />
        <Slider label="Tune (st)" value={spectral.tune} min={-24} max={24} step={0.5} onChange={(v) => patchSpectral({ tune: v })} />
        <Slider label="Volume (%)" value={spectral.volume} min={0} max={100} step={1} onChange={(v) => patchSpectral({ volume: v })} />
        <Slider label="Formant (st)" value={spectral.formantShift} min={-12} max={12} step={0.5} onChange={(v) => patchSpectral({ formantShift: v })} />
      </div>

      {needsB && (
        <>
          <h3>Sample B</h3>
          <div className="row wrap">
            <label>Source</label>
            <select
              value={spectral.sourceIndex2 === null ? "" : String(spectral.sourceIndex2)}
              onChange={(e) => patchSpectral({ sourceIndex2: e.target.value === "" ? null : Number(e.target.value) })}
            >
              <option value="">Unassigned</option>
              {props.sampleNames.map((n, slot) =>
                n ? (
                  <option key={slot} value={String(slot)}>
                    {slot}: {n}
                  </option>
                ) : null,
              )}
            </select>
          </div>
          <Waveform
            peaks={sourceBWave}
            freezePoint={spectral.freezePointB}
            onFreezeChange={(v) => patchSpectral({ freezePointB: v })}
            height={56}
            emptyLabel="Assign Sample B"
          />
          <div className="row wrap">
            <Slider label="Freeze Point B (%)" value={spectral.freezePointB} min={0} max={100} step={1} onChange={(v) => patchSpectral({ freezePointB: v })} />
            <Slider label="Tune B (st)" value={spectral.tuneB} min={-24} max={24} step={0.5} onChange={(v) => patchSpectral({ tuneB: v })} />
            <Slider label="Volume B (%)" value={spectral.volumeB} min={0} max={100} step={1} onChange={(v) => patchSpectral({ volumeB: v })} />
            <Slider label="Formant B (st)" value={spectral.formantShiftB} min={-12} max={12} step={0.5} onChange={(v) => patchSpectral({ formantShiftB: v })} />
          </div>
        </>
      )}

      {spectralModeHasAmount(spectral.mode) && (
        <Slider
          label={`${spectralModeLabel(spectral.mode)} Amount (%)`}
          value={amount}
          min={0}
          max={100}
          step={1}
          onChange={(v) => patchSpectral({ [amountKey]: v } as Partial<typeof spectral>)}
        />
      )}
      <div className="row wrap">
        <Slider label="Stereo Width (%)" value={spectral.stereoWidth} min={0} max={100} step={1} onChange={(v) => patchSpectral({ stereoWidth: v })} />
        <Slider label="Loop Length (s)" value={spectral.loopLengthSeconds} min={MIN_LOOP_SECONDS} max={MAX_LOOP_SECONDS} step={0.1} onChange={(v) => patchSpectral({ loopLengthSeconds: v })} />
      </div>

      <h3>Result</h3>
      <Waveform
        peaks={resultWave}
        markers={resultMarkers}
        height={72}
        emptyLabel={needsB && spectral.sourceIndex2 === null ? "Choose Sample B to render" : "Not rendered yet"}
      />

      <EnvelopeControls settings={settings} onUpdate={props.onUpdate} />
    </div>
  );
}

function EnvelopeControls(props: {
  settings: SamplerSettings;
  onUpdate: (patch: Partial<SamplerSettings>) => void;
}) {
  const { settings } = props;
  return (
    <>
      <h3>ENVELOPE · drag the points or adjust the values</h3>
      <AdsrGraph settings={settings} onChange={props.onUpdate} />
      <div className="row wrap">
        <Slider label="Attack (s)" value={settings.attack} min={0} max={5} step={0.001} onChange={(v) => props.onUpdate({ attack: v })} />
        <Slider label="Decay (s)" value={settings.decay} min={0} max={5} step={0.001} onChange={(v) => props.onUpdate({ decay: v })} />
        <Slider label="Sustain" value={settings.sustain} min={0} max={1} step={0.01} onChange={(v) => props.onUpdate({ sustain: v })} />
        <Slider label="Release (s)" value={settings.release} min={0} max={5} step={0.001} onChange={(v) => props.onUpdate({ release: v })} />
        <Slider label="Volume" value={settings.volume} min={0} max={1.5} step={0.01} onChange={(v) => props.onUpdate({ volume: v })} />
        <span className="mono muted">{formatDb(settings.volume)} dB</span>
      </div>
    </>
  );
}

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="slider">
      {props.label}
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
      <NumberInput
        value={props.value}
        min={props.min}
        max={props.max}
        step={props.step}
        onChange={props.onChange}
      />
    </label>
  );
}

function NumberField(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="slider">
      {props.label}
      <input
        type="number"
        step={props.step ?? 0.001}
        min={props.min}
        max={props.max}
        value={props.value}
        onChange={(e) =>
          props.onChange(Math.min(Math.max(Number(e.target.value), props.min), props.max))
        }
      />
    </label>
  );
}
