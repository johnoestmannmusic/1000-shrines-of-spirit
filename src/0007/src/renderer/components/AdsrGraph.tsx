import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";
import type { SamplerSettings } from "@/core/sampler";

const MAX_ATTACK = 5;
const MAX_DECAY = 5;
const MAX_RELEASE = 5;
const NOMINAL_HOLD = 0.3;
const AXIS_TOTAL = MAX_ATTACK + MAX_DECAY + NOMINAL_HOLD + MAX_RELEASE;
const PAD = 5;
const HIT_RADIUS = 14;

type DragKey = "attack" | "decay" | "release";

interface AdsrGraphProps {
  settings: SamplerSettings;
  onChange: (patch: Partial<SamplerSettings>) => void;
}

/**
 * ADSR envelope graph, recreated from the original 0006 implementation:
 * a fixed x-axis (each stage's max + a nominal sustain hold), 4px vertical
 * padding, an amber stroke with a translucent fill, and three draggable
 * points. The sustain plateau is drawn at a nominal width because a real
 * note holds it for as long as the pattern keeps the note down.
 */
export function AdsrGraph({ settings, onChange }: AdsrGraphProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const dragKey = useRef<DragKey | null>(null);
  const size = useRef({ width: 0, height: 0 });

  const points = () => {
    const a = settings.attack;
    const d = settings.decay;
    const s = settings.sustain;
    const r = settings.release;
    return {
      attack: { t: a, level: 1 },
      decay: { t: a + d, level: s },
      release: { t: a + d + NOMINAL_HOLD + r, level: 0 },
    };
  };

  const draw = () => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const cssWidth = wrap.clientWidth || 300;
    const cssHeight = 110;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.floor(cssWidth * dpr));
    canvas.height = Math.max(1, Math.floor(cssHeight * dpr));
    canvas.style.width = `${cssWidth}px`;
    canvas.style.height = `${cssHeight}px`;
    size.current = { width: cssWidth, height: cssHeight };

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const styles = getComputedStyle(document.documentElement);
    const bg = styles.getPropertyValue("--deep").trim() || "#0a0a0b";
    const accent = styles.getPropertyValue("--accent").trim() || "#6cd73c";

    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, cssWidth, cssHeight);

    const h = cssHeight;
    const xAt = (t: number) => (t / AXIS_TOTAL) * cssWidth;
    const yAt = (level: number) => h - PAD - level * (h - 2 * PAD);

    const pts = points();
    const holdEnd = pts.attack.t + settings.decay + NOMINAL_HOLD;

    // Curve
    ctx.beginPath();
    ctx.moveTo(xAt(0), yAt(0));
    ctx.lineTo(xAt(pts.attack.t), yAt(1));
    ctx.lineTo(xAt(pts.decay.t), yAt(pts.decay.level));
    ctx.lineTo(xAt(holdEnd), yAt(pts.decay.level));
    ctx.lineTo(xAt(pts.release.t), yAt(0));
    ctx.strokeStyle = accent;
    ctx.lineWidth = 2;
    ctx.stroke();

    // Translucent fill under the curve
    ctx.lineTo(xAt(0), yAt(0));
    ctx.closePath();
    ctx.fillStyle = hexToRgba(accent, 0.15);
    ctx.fill();

    // Draggable points
    (["attack", "decay", "release"] as DragKey[]).forEach((key) => {
      const point = pts[key];
      const active = dragKey.current === key;
      ctx.beginPath();
      ctx.arc(xAt(point.t), yAt(point.level), active ? 6 : 4.5, 0, Math.PI * 2);
      ctx.fillStyle = active ? lighten(accent) : accent;
      ctx.fill();
      ctx.lineWidth = 1;
      ctx.strokeStyle = bg;
      ctx.stroke();
    });
  };

  useEffect(() => {
    draw();
    const wrap = wrapRef.current;
    if (!wrap || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => draw());
    observer.observe(wrap);
    return () => observer.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.attack, settings.decay, settings.sustain, settings.release, settings.volume]);

  const localPoint = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  const applyDrag = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const key = dragKey.current;
    if (!key) return;
    const { x, y } = localPoint(event);
    const h = size.current.height;
    const t = Math.min(Math.max((x / Math.max(size.current.width, 1)) * AXIS_TOTAL, 0), AXIS_TOTAL);
    const level = Math.min(Math.max((h - PAD - y) / (h - 2 * PAD), 0), 1);
    if (key === "attack") {
      onChange({ attack: Math.min(Math.max(t, 0), MAX_ATTACK) });
    } else if (key === "decay") {
      onChange({
        decay: Math.min(Math.max(t - settings.attack, 0), MAX_DECAY),
        sustain: level,
      });
    } else {
      onChange({
        release: Math.min(
          Math.max(t - settings.attack - settings.decay - NOMINAL_HOLD, 0),
          MAX_RELEASE,
        ),
      });
    }
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const { x, y } = localPoint(event);
    const h = size.current.height;
    const xAt = (t: number) => (t / AXIS_TOTAL) * size.current.width;
    const yAt = (level: number) => h - PAD - level * (h - 2 * PAD);
    const pts = points();
    const candidates: Array<[DragKey, { t: number; level: number }]> = [
      ["attack", pts.attack],
      ["decay", pts.decay],
      ["release", pts.release],
    ];
    let best: DragKey | null = null;
    let bestDist = Infinity;
    for (const [key, point] of candidates) {
      const dist = Math.hypot(x - xAt(point.t), y - yAt(point.level));
      if (dist < bestDist) {
        bestDist = dist;
        best = key;
      }
    }
    if (best && bestDist <= HIT_RADIUS) {
      dragKey.current = best;
      event.currentTarget.setPointerCapture(event.pointerId);
      applyDrag(event);
    }
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (!dragKey.current) return;
    applyDrag(event);
  };

  const onPointerUp = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (dragKey.current) {
      dragKey.current = null;
      try {
        event.currentTarget.releasePointerCapture(event.pointerId);
      } catch {
        /* ignore */
      }
      draw();
    }
  };

  return (
    <div className="adsr-wrap" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        className="adsr-canvas"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        title="Attack / decay / sustain / release — drag the points to adjust (the sustain plateau is shown at a nominal width; real hold time depends on the note)"
      />
    </div>
  );
}

function hexToRgba(hex: string, alpha: number): string {
  const clean = hex.replace("#", "").trim();
  if (clean.length !== 6) return `rgba(201,151,58,${alpha})`;
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

function lighten(hex: string): string {
  const clean = hex.replace("#", "").trim();
  if (clean.length !== 6) return "#ffd27a";
  const r = Math.min(255, parseInt(clean.slice(0, 2), 16) + 60);
  const g = Math.min(255, parseInt(clean.slice(2, 4), 16) + 60);
  const b = Math.min(255, parseInt(clean.slice(4, 6), 16) + 60);
  return `rgb(${r},${g},${b})`;
}
