import { useRef, useState } from "react";
import type { SongModel } from "@/core/songModel";
import { songPositionAt } from "@/core/timing";
import type { AudioBackend, PlaybackMode } from "@/audio/backend";
import { useAnimationFrame } from "../hooks";

interface TransportProps {
  backend: AudioBackend;
  song: SongModel;
  mode: PlaybackMode;
  stemsAvailable: boolean;
  editMode: boolean;
  onModeChange: (mode: PlaybackMode) => void;
  onToggleEdit: () => void;
}

export function Transport({
  backend,
  song,
  mode,
  stemsAvailable,
  editMode,
  onModeChange,
  onToggleEdit,
}: TransportProps) {
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [pos, setPos] = useState({ orderPos: 0, row: 0 });
  const [ready, setReady] = useState(false);

  useAnimationFrame(() => {
    setPlaying(backend.isPlaying());
    const t = backend.currentTime();
    setTime(t);
    setDuration(backend.songDuration());
    setPos(songPositionAt(song, t));
    setReady(mode === "chip" ? backend.stemsReady() : backend.samplerReady());
  }, 20);

  const play = () => {
    backend.ensureStarted();
    if (backend.isPlaying()) backend.pause();
    else backend.play(backend.currentTime());
  };

  return (
    <section className="panel transport">
      <div className="row wrap">
        <span
          className="mode-tip"
          title={
            stemsAvailable
              ? "Play the original Furnace-rendered stems"
              : "CHIP MODE needs this song's four stem files, which aren't available for this project."
          }
        >
          <button
            className="mode-btn chip"
            data-active={mode === "chip"}
            disabled={!stemsAvailable}
            onClick={() => onModeChange("chip")}
          >
            CHIP MODE
          </button>
        </span>
        <button
          className="mode-btn sampler"
          data-active={mode === "sampler" && !editMode}
          onClick={() => onModeChange("sampler")}
        >
          SAMPLER MODE
        </button>
        <button
          className="mode-btn edit"
          data-active={editMode}
          onClick={onToggleEdit}
        >
          EDIT MODE
        </button>
        <span className="spacer" />
        <span className="status">
          {ready ? "" : mode === "chip" ? "Decoding stems…" : "Decoding samples…"}
        </span>
      </div>

      <div className="row transport-controls">
        <button className="play" disabled={!ready} onClick={play}>
          {playing ? "❚❚ Pause" : "▶ Play"}
        </button>
        <button disabled={!ready} onClick={() => backend.stop()}>
          ■ Stop
        </button>
        <input
          className="seek"
          type="range"
          min={0}
          max={Math.max(duration, 0.001)}
          step={0.01}
          value={Math.min(time, duration)}
          onChange={(e) => backend.seek(Number(e.target.value))}
        />
        <span className="mono position">
          {time.toFixed(1)}s / {duration.toFixed(1)}s · pattern {pos.orderPos}, row{" "}
          {pos.row.toString().padStart(2, "0")}
        </span>
      </div>
    </section>
  );
}
