import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";

export interface WaveformMarker {
  fraction: number;
  color: string;
  label?: number;
  alpha: number;
}

interface WaveformProps {
  peaks: Array<[number, number]>;
  height?: number;
  color?: string;
  markers?: WaveformMarker[];
  /** Dim everything outside [start,end] seconds when `duration` is set. */
  trim?: { start: number; end: number; duration: number };
  onTrimChange?: (start: number, end: number) => void;
  /** Draw and drag a vertical line at this 0-100 value. */
  freezePoint?: number;
  onFreezeChange?: (value: number) => void;
  onScrub?: (fraction: number) => void;
  emptyLabel?: string;
}

export function Waveform({
  peaks,
  height = 56,
  color,
  markers = [],
  trim,
  onTrimChange,
  freezePoint,
  onFreezeChange,
  onScrub,
  emptyLabel,
}: WaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;
    const width = parent.clientWidth;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.floor(width * dpr));
    canvas.height = Math.max(1, Math.floor(height * dpr));
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const styles = getComputedStyle(document.documentElement);
    const waveColor = color || styles.getPropertyValue("--wave").trim() || "#6cd73c";
    const deep = styles.getPropertyValue("--deep").trim() || "#0a0a0b";
    ctx.fillStyle = deep;
    ctx.fillRect(0, 0, width, height);

    if (peaks.length === 0) {
      if (emptyLabel) {
        ctx.fillStyle = styles.getPropertyValue("--secondary").trim() || "#888";
        ctx.font = "13px Medodica, system-ui, sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(emptyLabel, width / 2, height / 2);
      }
      return;
    }

    const center = height / 2;
    const amp = height * 0.44;
    ctx.strokeStyle = waveColor;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 0; i < peaks.length; i++) {
      const x = (i / peaks.length) * width + 0.5;
      const [low, high] = peaks[i]!;
      ctx.moveTo(x, center - high * amp);
      ctx.lineTo(x, center - low * amp);
    }
    ctx.stroke();

    if (trim && trim.duration > 0) {
      const xFor = (t: number) => (Math.min(Math.max(t / trim.duration, 0), 1) * width);
      ctx.fillStyle = "rgba(0,0,0,0.35)";
      ctx.fillRect(0, 0, xFor(trim.start), height);
      ctx.fillRect(xFor(trim.end), 0, width - xFor(trim.end), height);
      ctx.strokeStyle = styles.getPropertyValue("--accent").trim() || "#6cd73c";
      ctx.lineWidth = 2;
      for (const x of [xFor(trim.start), xFor(trim.end)]) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, height);
        ctx.stroke();
      }
    }

    if (freezePoint !== undefined) {
      const x = (Math.min(Math.max(freezePoint, 0), 100) / 100) * width;
      ctx.strokeStyle = styles.getPropertyValue("--accent").trim() || "#6cd73c";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }

    for (const marker of markers) {
      if (marker.fraction < 0 || marker.fraction > 1) continue;
      const x = marker.fraction * width;
      ctx.globalAlpha = Math.min(Math.max(marker.alpha, 0.15), 1);
      ctx.strokeStyle = marker.color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }, [peaks, height, color, markers, trim, freezePoint, emptyLabel]);

  const fractionFromEvent = (event: ReactPointerEvent<HTMLDivElement>): number => {
    const rect = event.currentTarget.getBoundingClientRect();
    return Math.min(Math.max((event.clientX - rect.left) / Math.max(rect.width, 1), 0), 1);
  };

  let interaction: "freeze" | "trim" | "scrub" | "none" = "none";
  if (freezePoint !== undefined && onFreezeChange) interaction = "freeze";
  else if (trim && onTrimChange) interaction = "trim";
  else if (onScrub) interaction = "scrub";

  const handlePointer = (event: ReactPointerEvent<HTMLDivElement>) => {
    const fraction = fractionFromEvent(event);
    if (interaction === "freeze") onFreezeChange!(fraction * 100);
    else if (interaction === "scrub") onScrub!(fraction);
    else if (interaction === "trim" && trim) {
      const time = fraction * trim.duration;
      const nearStart = Math.abs(fraction - trim.start / trim.duration);
      const nearEnd = Math.abs(fraction - trim.end / trim.duration);
      if (nearStart <= nearEnd) onTrimChange!(Math.min(time, trim.end), trim.end);
      else onTrimChange!(trim.start, Math.max(time, trim.start));
    }
  };

  const interactive = interaction !== "none";

  return (
    <div
      className="waveform"
      style={{ height, cursor: interactive ? "crosshair" : "default" }}
      onPointerDown={(e) => {
        if (!interactive) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        handlePointer(e);
      }}
      onPointerMove={(e) => {
        if (!interactive) return;
        if (e.buttons === 0) return;
        handlePointer(e);
      }}
    >
      <canvas ref={canvasRef} style={{ width: "100%", height: `${height}px` }} />
    </div>
  );
}
