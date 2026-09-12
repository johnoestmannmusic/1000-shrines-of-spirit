import { useRef, useState, useEffect } from "react";
import type { AudioBackend } from "@/audio/backend";
import { useAnimationFrame } from "../hooks";
import { useExplainer } from "../explainer";
import { mixerChannelExplain, mixerExplain, mixerMasterExplain } from "../explainerContent";
import { dbToLinear, formatDb } from "../util";

const CHANNEL_NAMES = ["Pulse 1", "Pulse 2", "Wave", "Noise"];

interface MixerProps {
  backend: AudioBackend;
  channelVolume: number[];
  channelMuted: boolean[];
  masterVolume: number;
  onChannelVolume: (channel: number, volume: number) => void;
  onChannelMute: (channel: number, muted: boolean) => void;
  onMasterVolume: (volume: number) => void;
}

function linearToDb(value: number): number {
  return value > 0 ? 20 * Math.log10(value) : -Infinity;
}

function DbField({
  value,
  onChange,
}: {
  value: number;
  onChange: (db: number) => void;
}) {
  const [text, setText] = useState(() => formatDb(value));
  const focused = useRef(false);

  useEffect(() => {
    if (!focused.current) setText(formatDb(value));
  }, [value]);

  const commit = () => {
    const trimmed = text.trim().toLowerCase();
    if (trimmed === "-inf" || trimmed === "-infinity" || trimmed === "-∞") {
      onChange(0);
      setText("-∞");
      return;
    }
    const parsed = Number(trimmed);
    if (Number.isFinite(parsed)) {
      onChange(Math.min(Math.max(dbToLinear(parsed), 0), 1.5));
    } else {
      setText(formatDb(value));
    }
  };

  return (
    <input
      className="db-input mono"
      value={text}
      onFocus={() => {
        focused.current = true;
      }}
      onBlur={() => {
        focused.current = false;
        commit();
      }}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

function Meter({ level, master = false }: { level: number; master?: boolean }) {
  const db = level > 0 ? 20 * Math.log10(level) : -Infinity;
  const width = `${Math.min(level, 1) * 100}%`;
  return (
    <div className={`meter${master ? " master" : ""}`} title={`${db.toFixed(1)} dB`}>
      <div className={`meter-fill${level >= 0.98 ? " clip" : ""}`} style={{ width }} />
    </div>
  );
}

export function Mixer(props: MixerProps) {
  const [meters, setMeters] = useState([0, 0, 0, 0, 0]);
  const hold = useRef([0, 0, 0, 0, 0]);
  const explain = useExplainer();

  useAnimationFrame(() => {
    const levels = props.backend.meterLevels();
    const h = hold.current;
    for (let i = 0; i < h.length; i++) {
      h[i] = Math.max(levels[i] ?? 0, (h[i] ?? 0) * 0.85);
    }
    setMeters(h.slice());
  }, 20);

  return (
    <section className="panel">
      <h2 style={{ cursor: "help" }} onMouseEnter={() => explain(mixerExplain())}>MIXER</h2>
      {[0, 1, 2, 3].map((c) => (
        <div
          className="row mixer-row"
          key={c}
          onMouseEnter={() => explain(mixerChannelExplain(c))}
        >
          <button
            className={`mute-btn${props.channelMuted[c] ? " muted" : ""}`}
            title={`${CHANNEL_NAMES[c]} — ${props.channelMuted[c] ? "click to unmute" : "click to mute"}`}
            onClick={() => props.onChannelMute(c, !(props.channelMuted[c] ?? false))}
          >
            CH{c}
          </button>
          <input
            type="range"
            min={0}
            max={1.5}
            step={0.01}
            value={props.channelVolume[c] ?? 1}
            onChange={(e) => props.onChannelVolume(c, Number(e.target.value))}
          />
          <DbField
            value={props.channelVolume[c] ?? 1}
            onChange={(db) => props.onChannelVolume(c, db)}
          />
          <span className="db">dB</span>
          <Meter level={meters[c] ?? 0} />
        </div>
      ))}
      <div
        className="row mixer-row master-row"
        onMouseEnter={() => explain(mixerMasterExplain())}
      >
        <span className="mute">MASTER</span>
        <input
          type="range"
          min={0}
          max={1.5}
          step={0.01}
          value={props.masterVolume}
          onChange={(e) => props.onMasterVolume(Number(e.target.value))}
        />
        <DbField value={props.masterVolume} onChange={props.onMasterVolume} />
        <span className="db">dB</span>
        <Meter level={meters[4] ?? 0} master />
      </div>
    </section>
  );
}
