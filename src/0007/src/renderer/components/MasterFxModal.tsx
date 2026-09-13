import { DraggableModal } from "./DraggableModal";
import { NumberInput } from "./NumberInput";
import { ps1EchoPreset, type MasterFxSettings } from "@/core/masterFx";

interface MasterFxModalProps {
  settings: MasterFxSettings;
  onChange: (settings: MasterFxSettings) => void;
  onClose: () => void;
}

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
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
      {props.suffix ? <span className="muted small">{props.suffix}</span> : null}
    </label>
  );
}

export function MasterFxModal({ settings, onChange, onClose }: MasterFxModalProps) {
  const delay = settings.delay;
  const reverb = settings.reverb;
  const setDelay = (patch: Partial<typeof delay>) =>
    onChange({ ...settings, delay: { ...delay, ...patch } });
  const setReverb = (patch: Partial<typeof reverb>) =>
    onChange({ ...settings, reverb: { ...reverb, ...patch } });

  return (
    <DraggableModal title="Master FX" onClose={onClose} width={460}>
      <p className="hint">
        Effects on the master output. Both are off by default; the PS1 Echo preset recreates the
        PlayStation's short, dark echo.
      </p>
      <div className="row">
        <button onClick={() => onChange(ps1EchoPreset())}>PS1 Echo Preset</button>
      </div>

      <div className="row">
        <label>
          <input
            type="checkbox"
            checked={delay.enabled}
            onChange={(e) => setDelay({ enabled: e.target.checked })}
          />
          Delay
        </label>
      </div>
      <Slider
        label="Time (s)"
        value={delay.timeSec}
        min={0.01}
        max={1.0}
        step={0.01}
        onChange={(v) => setDelay({ timeSec: v })}
      />
      <Slider
        label="Feedback"
        value={delay.feedback}
        min={0}
        max={0.95}
        step={0.01}
        onChange={(v) => setDelay({ feedback: v })}
      />
      <Slider
        label="Tone (Hz)"
        value={delay.toneHz}
        min={400}
        max={8000}
        step={10}
        onChange={(v) => setDelay({ toneHz: v })}
      />
      <Slider
        label="Mix"
        value={delay.mix}
        min={0}
        max={1}
        step={0.01}
        onChange={(v) => setDelay({ mix: v })}
      />

      <div className="row" style={{ marginTop: 8 }}>
        <label>
          <input
            type="checkbox"
            checked={reverb.enabled}
            onChange={(e) => setReverb({ enabled: e.target.checked })}
          />
          Reverb
        </label>
      </div>
      <Slider
        label="Decay (s)"
        value={reverb.decaySec}
        min={0.2}
        max={8}
        step={0.1}
        onChange={(v) => setReverb({ decaySec: v })}
      />
      <Slider
        label="Mix"
        value={reverb.mix}
        min={0}
        max={1}
        step={0.01}
        onChange={(v) => setReverb({ mix: v })}
      />
    </DraggableModal>
  );
}
