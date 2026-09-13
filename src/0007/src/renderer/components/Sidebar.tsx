import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { SongModel } from "@/core/songModel";
import type { ProjectFile } from "@/core/project";
import { rowDurationSec } from "@/core/timing";
import { DEFAULT_EXPLAINER, useExplainer, type ExplainerContent } from "../explainer";
import { commentsExplain, timingExplain } from "../explainerContent";
import { NumberInput } from "./NumberInput";
import { LinkifiedText } from "./LinkifiedText";

const MAX_EXPLAIN_CHARS = 320;

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** Wrap numbers / identifiers / ALL-CAPS keywords in colour spans. */
function highlight(text: string): string {
  return escapeHtml(text).replace(
    /(\d+(?:\.\d+)?)|([A-Z][A-Z0-9 /-]{2,}?)(?=\s|$|[.,;:)]|\/)|([a-zA-Z]+_[a-zA-Z0-9_]+|[a-z]+[A-Z][a-zA-Z0-9]*)/g,
    (match, num, kw, ident) => {
      if (num) return `<span class="tk-num">${num}</span>`;
      if (kw) return `<span class="tk-kw">${kw}</span>`;
      if (ident) return `<span class="tk-var">${ident}</span>`;
      return match;
    },
  );
}

function renderExplainer(body: string): ReactNode[] {
  const lines = body.split("\n").filter((line) => line.trim().length > 0);
  const blocks: ReactNode[] = [];
  let used = 0;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (used + line.length > MAX_EXPLAIN_CHARS && blocks.length > 0) break;
    used += line.length;
    const html = highlight(line);
    const isCode = line.includes(" = ") && line.length <= 90;
    blocks.push(
      isCode ? (
        <div key={i} className="tk-block" dangerouslySetInnerHTML={{ __html: html }} />
      ) : (
        <p key={i} dangerouslySetInnerHTML={{ __html: html }} />
      ),
    );
  }
  return blocks;
}

export function ExplainerCard({ content }: { content: ExplainerContent }) {
  return (
    <section className="panel explainer">
      <strong className="explainer-title">{content.title}</strong>
      <div className="explainer-body">{renderExplainer(content.body)}</div>
    </section>
  );
}

function useHoverExplain(content: ExplainerContent) {
  const explain = useExplainer();
  return (event: React.MouseEvent) => explain(content);
}

export function SongComments({
  comments,
  fallback = "",
  editMode,
  onChange,
}: {
  comments: string;
  fallback?: string;
  editMode: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(true);
  const explain = useExplainer();
  const display = comments.trim() ? comments : fallback;
  return (
    <section className="panel" onMouseEnter={() => explain(commentsExplain())}>
      <button className="collapse-header" onClick={() => setOpen((o) => !o)}>
        {open ? "▾" : "▸"} SONG COMMENTS
      </button>
      {open &&
        (editMode ? (
          <textarea
            className="comments-edit"
            value={comments}
            placeholder="Song comments…"
            onChange={(e) => onChange(e.target.value)}
          />
        ) : (
          <p className="small linkified">
            <LinkifiedText text={display || "—"} />
          </p>
        ))}
    </section>
  );
}

export interface TimingEditPatch {
  tickRate?: number;
  speed?: number;
  highlightA?: number;
  highlightB?: number;
  virtualTempo?: [number, number];
}

export function TimingCard({
  song,
  editMode,
  onEdit,
}: {
  song: SongModel;
  editMode: boolean;
  onEdit: (patch: TimingEditPatch) => void;
}) {
  const rowDuration = rowDurationSec(song.meta);
  const bpm = 60 / (Math.max(song.meta.highlightA, 1) * rowDuration);
  const [open, setOpen] = useState(false);
  const [bpmText, setBpmText] = useState(() => bpm.toFixed(2));
  const bpmFocused = useRef(false);
  useEffect(() => {
    if (!bpmFocused.current) setBpmText(bpm.toFixed(2));
  }, [bpm]);
  const commitBpm = () => {
    const parsed = Number(bpmText);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      setBpmText(bpm.toFixed(2));
      return;
    }
    const speed = meta.speedPattern[0] ?? 6;
    const tickRate = Math.min(
      Math.max((parsed * Math.max(meta.highlightA, 1) * speed) / 60, 1),
      1000,
    );
    onEdit({ tickRate });
  };
  const onHover = useHoverExplain(timingExplain(song));
  const meta = song.meta;

  const number = (
    label: string,
    value: number,
    min: number,
    max: number,
    onChange: (v: number) => void,
    step = 1,
  ) => (
    <div className="timing-row">
      <span className="muted small">{label}</span>
      {editMode ? (
        <NumberInput value={value} min={min} max={max} step={step} onChange={onChange} />
      ) : (
        <span className="mono small">{value}</span>
      )}
    </div>
  );

  return (
    <section className="panel" onMouseEnter={onHover}>
      <button className="collapse-header" onClick={() => setOpen((o) => !o)}>
        {open ? "▾" : "▸"} TIMING · {bpm.toFixed(1)} BPM · {meta.tickRate.toFixed(1)} Hz
      </button>
      {open && (
        <div className="timing-grid">
          <div className="timing-row">
            <span className="muted small">base tempo (BPM)</span>
            {editMode ? (
              <input
                type="text"
                inputMode="decimal"
                value={bpmText}
                onFocus={() => {
                  bpmFocused.current = true;
                }}
                onBlur={() => {
                  bpmFocused.current = false;
                  commitBpm();
                }}
                onChange={(e) => setBpmText(e.target.value)}
                onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
                  if (e.key === "Enter") e.currentTarget.blur();
                }}
              />
            ) : (
              <span className="mono small">{bpm.toFixed(2)} BPM</span>
            )}
          </div>
          {number("tick rate (Hz)", meta.tickRate, 1, 1000, (v) => onEdit({ tickRate: v }), 1)}
          {number("speed(s)", meta.speedPattern[0] ?? 6, 1, 255, (v) => onEdit({ speed: v }))}
          <div className="timing-row">
            <span className="muted small">virtual tempo</span>
            {editMode ? (
              <span className="row">
                <NumberInput
                  value={meta.virtualTempo[0]}
                  min={1}
                  max={255}
                  step={1}
                  onChange={(v) => onEdit({ virtualTempo: [v, meta.virtualTempo[1]] })}
                />
                <span>/</span>
                <NumberInput
                  value={meta.virtualTempo[1]}
                  min={1}
                  max={255}
                  step={1}
                  onChange={(v) => onEdit({ virtualTempo: [meta.virtualTempo[0], v] })}
                />
              </span>
            ) : (
              <span className="mono small">
                {meta.virtualTempo[0]} / {meta.virtualTempo[1]}
              </span>
            )}
          </div>
          <div className="timing-row">
            <span className="muted small">highlights</span>
            {editMode ? (
              <span className="row">
                <NumberInput
                  value={meta.highlightA}
                  min={1}
                  max={255}
                  step={1}
                  onChange={(v) => onEdit({ highlightA: v })}
                />
                <NumberInput
                  value={meta.highlightB}
                  min={1}
                  max={255}
                  step={1}
                  onChange={(v) => onEdit({ highlightB: v })}
                />
              </span>
            ) : (
              <span className="mono small">
                {meta.highlightA}, {meta.highlightB}
              </span>
            )}
          </div>
          <div className="timing-row">
            <span className="muted small">patterns</span>
            <span className="mono small">
              {meta.orderLength} × {meta.patternLength} rows
            </span>
          </div>
          <div className="timing-row">
            <span className="muted small">channels</span>
            <span className="mono small">{song.channels.length}</span>
          </div>
        </div>
      )}
    </section>
  );
}

export function LicensesCard({ project }: { project: ProjectFile | null }) {
  if (!project) return null;
  return (
    <section className="panel">
      <h2>LICENSES</h2>
      <div className="small muted">Music License</div>
      <div className="small linkified">
        <LinkifiedText text={project.musicLicense || "—"} />
      </div>
      <div className="small muted" style={{ marginTop: 6 }}>
        Code License
      </div>
      <div className="small linkified">
        <LinkifiedText text={project.codeLicense || "—"} />
      </div>
      {(project.viewSourceLink || project.websiteLink) && (
        <div className="small linkified" style={{ marginTop: 6 }}>
          {project.viewSourceLink && (
            <div>
              Source: <LinkifiedText text={project.viewSourceLink} />
            </div>
          )}
          {project.websiteLink && (
            <div>
              Website: <LinkifiedText text={project.websiteLink} />
            </div>
          )}
        </div>
      )}
    </section>
  );
}

export function SongMetaCard({
  project,
  song,
  editMode,
  mode,
  anySpectral,
  onEdit,
}: {
  project: ProjectFile | null;
  song: SongModel;
  editMode: boolean;
  mode: "chip" | "sampler";
  anySpectral: boolean;
  onEdit: (patch: Partial<ProjectFile>) => void;
}) {
  const onHover = useHoverExplain({
    title: "SONG",
    body: "Song metadata from the Project JSON. In EDIT MODE the Title, Artist and Album fields can be edited and are saved with the project.",
  });
  if (!project) return null;
  return (
    <section className="panel" onMouseEnter={onHover}>
      <h2>SONG</h2>
      {editMode ? (
        <div className="row wrap">
          <label>
            Title
            <input value={project.songTitle} onChange={(e) => onEdit({ songTitle: e.target.value })} />
          </label>
          <label>
            Artist
            <input value={project.artist} onChange={(e) => onEdit({ artist: e.target.value })} />
          </label>
          <label>
            Album
            <input value={project.album} onChange={(e) => onEdit({ album: e.target.value })} />
          </label>
        </div>
      ) : (
        <p className="mono small">
          {project.songTitle || song.meta.name}
          {project.artist ? ` — ${project.artist}` : ""}
          {project.album ? ` (${project.album})` : ""}
        </p>
      )}
      <p className="small muted">
        {mode === "chip"
          ? "Game Boy"
          : anySpectral
            ? "SAMPLER / SPECTRALPRISM"
            : "SAMPLER"}{" "}
        · {song.meta.orderLength} patterns · {song.meta.patternLength} rows
      </p>
    </section>
  );
}

export { DEFAULT_EXPLAINER };
