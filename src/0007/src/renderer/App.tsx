import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import {
  applySnapshot,
  buildSongModel,
  buildSongModelFromProject,
  cellAt,
  instrumentColor,
  patternSnapshot,
  retime,
  type InstrumentInfo,
  type SongModel,
} from "@/core/songModel";
import { clearPatternsSnapshot, reassignInstrument, remapInstrumentsAfterDelete } from "@/core/tracker";
import { defaultMasterFx, type MasterFxSettings } from "@/core/masterFx";
import {
  applyTimingOverrides,
  defaultProject,
  projectFromJson,
  projectToJson,
  projectToValue,
  validateProject,
  type ProjectFile,
} from "@/core/project";
import {
  defaultSamplerSettings,
  sequenceFromSong,
  setSpectralEnabled,
  type SamplerSettings,
} from "@/core/sampler";
import {
  applyExportEnvelope,
  arrangeExport,
  renderSamplerMix,
  wavPcm16,
  zipStore,
  type ExportParams,
} from "@/core/export";
import { clipLen, clipSlice, type AudioClip } from "@/core/dsp";
import { writeMidi } from "@/core/midi";
import { samplerPlaybackRate } from "@/core/pitch";
import { rowDuration, rowTime } from "@/core/timing";
import { WebAudioBackend } from "@/audio/webAudioBackend";
import { applyMasterFxOffline, decodeAudioBytes } from "@/audio/offline";
import type { PlaybackMode } from "@/audio/backend";
import { applyTheme, loadTheme, type ThemeName } from "./theme";
import { safeFilename } from "./util";
import { Toolbar } from "./components/Toolbar";
import { Transport } from "./components/Transport";
import { Mixer } from "./components/Mixer";
import { InstrumentList } from "./components/InstrumentList";
import { PatternGrid } from "./components/PatternGrid";
import { SourceSamples } from "./components/SourceSamples";
import { Piano } from "./components/Piano";
import { CoverArt, type CoverArtHandle } from "./components/CoverArt";
import { SamplerEditor } from "./components/SamplerEditor";
import { DraggableModal } from "./components/DraggableModal";
import { MasterFxModal } from "./components/MasterFxModal";
import { WavExportModal } from "./components/WavExportModal";
import { AudioError } from "./components/AudioError";
import {
  ExplainerCard,
  LicensesCard,
  SongComments,
  SongMetaCard,
  TimingCard,
  type TimingEditPatch,
} from "./components/Sidebar";
import {
  DEFAULT_EXPLAINER,
  ExplainerContext,
  type ExplainerContent,
} from "./explainer";
import { loadLocalState, saveLocalState } from "./localState";
import { initPrismWasm } from "./vendor/prism/loader";
import { chooseAudioFile, loadDefaultSong, saveFile } from "./platform";

interface EditorState {
  index: number;
  spectral: boolean;
}

function findInstrumentSpot(
  song: SongModel,
  instrument: number,
): { channel: number; order: number; row: number } | null {
  const channelCount = Math.min(song.channels.length, 4);
  for (let order = 0; order < song.meta.orderLength; order++) {
    for (let row = 0; row < song.meta.patternLength; row++) {
      for (let channel = 0; channel < channelCount; channel++) {
        const ch = song.channels[channel]!;
        const ins = ch.insTimeline[order]?.[row];
        const note = ch.noteTimeline[order]?.[row];
        if (ins === instrument && note && note.kind === "note") {
          return { channel, order, row };
        }
      }
    }
  }
  return null;
}

declare const __BUILD_DATE__: string | undefined;
const BUILD_NUMBER = `v${
  typeof __BUILD_DATE__ === "string"
    ? __BUILD_DATE__
    : new Date().toISOString().slice(0, 10).replace(/-/g, "")
}`;

const NEW_PROJECT_TITLE = "New Song";
const NEW_PROJECT_ARTIST = "Unknown Artist";
const NEW_PROJECT_ALBUM = "New Album";

export function App() {
  const backendRef = useRef<WebAudioBackend | null>(null);
  const coverRef = useRef<CoverArtHandle | null>(null);
  const songRef = useRef<SongModel | null>(null);
  const settingsRef = useRef<SamplerSettings[]>([]);
  const viewOrderRef = useRef(0);
  const referenceRef = useRef(false);
  const jsonFileInputRef = useRef<HTMLInputElement | null>(null);
  const selectionRef = useRef<{ order: number; row: number } | null>(null);
  const editModeRef = useRef(false);
  const [backend, setBackend] = useState<WebAudioBackend | null>(null);
  const [song, setSong] = useState<SongModel | null>(null);
  const [project, setProject] = useState<ProjectFile | null>(null);
  const [settings, setSettings] = useState<SamplerSettings[]>([]);
  const [sampleNames, setSampleNames] = useState<string[]>([]);
  const [mode, setMode] = useState<PlaybackMode>("sampler");
  const [editMode, setEditMode] = useState(false);
  const [stemsAvailable, setStemsAvailable] = useState(false);
  const [channelVolume, setChannelVolume] = useState([1, 1, 1, 1]);
  const [channelMuted, setChannelMuted] = useState([false, false, false, false]);
  const [masterVolume, setMasterVolume] = useState(1);
  const [reference, setReference] = useState(false);
  const [theme, setTheme] = useState<ThemeName>(() => loadTheme());
  const [status, setStatus] = useState("Loading bundled song…");
  const [error, setError] = useState<string | null>(null);

  const [currentFur, setCurrentFur] = useState<Uint8Array | null>(null);
  const [chipMix, setChipMix] = useState<Uint8Array | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [projectIoOpen, setProjectIoOpen] = useState(false);
  const [projectIoText, setProjectIoText] = useState("");
  const [sampleInfo, setSampleInfo] = useState<number | null>(null);
  const [sampleInfoName, setSampleInfoName] = useState("");
  const [sampleInfoComments, setSampleInfoComments] = useState("");
  const [clearConfirm, setClearConfirm] = useState(false);
  const [newProjectConfirm, setNewProjectConfirm] = useState(false);
  const [deletePrompt, setDeletePrompt] = useState<{
    index: number;
    mode: "remove" | "reassign";
    target: number;
  } | null>(null);
  const [wasmReady, setWasmReady] = useState(false);
  const [masterFx, setMasterFx] = useState<MasterFxSettings>(defaultMasterFx());
  const [masterFxOpen, setMasterFxOpen] = useState(false);
  const [wavExportOpen, setWavExportOpen] = useState(false);
  const [wavExportBusy, setWavExportBusy] = useState(false);
  const [wavExportProgress, setWavExportProgress] = useState(0);
  const [explainer, setExplainer] = useState<ExplainerContent>(DEFAULT_EXPLAINER);

  songRef.current = song;
  settingsRef.current = settings;
  referenceRef.current = reference;
  editModeRef.current = editMode;

  useEffect(() => {
    let cancelled = false;
    void initPrismWasm().then((ready) => {
      if (!cancelled) setWasmReady(ready);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (!song) return;
    saveLocalState({
      mutes: settings.map((s) => s.muted),
      instrumentNames: song.instruments.map((i) => i.name),
      instrumentColors: song.instruments.map((i) => i.colorRgb),
      reference,
    });
  }, [song, settings, reference]);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (
        target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT")
      ) {
        return;
      }
      if (event.code !== "Space") return;
      const engine = backendRef.current;
      if (!engine) return;
      event.preventDefault();
      if (event.ctrlKey) {
        engine.ensureStarted();
        const model = songRef.current;
        const selection = selectionRef.current;
        if (editModeRef.current && model && selection) {
          // Continue playback from the selected EDIT cell's row.
          const start = rowTime(model, selection.order, selection.row);
          if (engine.isPlaying()) engine.seek(start);
          else engine.play(start);
        } else if (!engine.isPlaying()) {
          engine.play(engine.currentTime());
        }
        return;
      }
      if (engine.isPlaying()) {
        engine.pause();
      } else {
        engine.ensureStarted();
        // Plain Space starts from the viewed pattern's first row.
        const model = songRef.current;
        const start = model ? rowTime(model, viewOrderRef.current, 0) : engine.currentTime();
        engine.play(start);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const applyLoaded = useCallback((result: Awaited<ReturnType<typeof loadDefaultSong>>) => {
    if ("error" in result && result.error) {
      setError(result.error);
      setStatus("");
      return;
    }
    if (!("project" in result)) return;
    const loadedProject = projectFromJson(result.project);
    let model: SongModel;
    if (result.raw) {
      model = buildSongModel(result.raw);
      if (loadedProject.patternSnapshot) applySnapshot(model, loadedProject.patternSnapshot);
      applyTimingOverrides(loadedProject, model);
    } else {
      // Project-only song (no Furnace .fur / CHIP MODE): rebuild the model
      // from the project's pattern snapshot and timing overrides.
      model = buildSongModelFromProject(loadedProject);
    }
    validateProject(loadedProject, model.instruments.length);
    loadedProject.instrumentNames.forEach((name, i) => {
      if (model.instruments[i]) model.instruments[i]!.name = name;
    });

    const loadedSettings = model.instruments.map(
      (_, i) => loadedProject.instruments[i] ?? defaultSamplerSettings(),
    );
    loadedProject.mutedInstruments.forEach((muted, i) => {
      if (loadedSettings[i]) loadedSettings[i]!.muted = !!muted;
    });

    const saved = loadLocalState();
    if (saved) {
      saved.mutes.forEach((m, i) => {
        if (loadedSettings[i]) loadedSettings[i]!.muted = !!m;
      });
      saved.instrumentNames.forEach((n, i) => {
        if (model.instruments[i] && n) model.instruments[i]!.name = n;
      });
      saved.instrumentColors.forEach((c, i) => {
        if (model.instruments[i] && c.length === 3)
          model.instruments[i]!.colorRgb = [c[0]!, c[1]!, c[2]!];
      });
    }

    const engine = new WebAudioBackend();
    engine.ensureStarted();
    for (let c = 0; c < 4; c++) {
      engine.setChannelVolume(c, loadedProject.channelVolume[c] ?? 1);
      engine.setChannelMute(c, loadedProject.mutedChannels[c] ?? false);
    }
    engine.setMasterVolume(loadedProject.masterVolume);
    engine.setMasterFx(loadedProject.masterFx);
    const sampleBytes: Array<Uint8Array | null> = Array.from(
      { length: 6 },
      (_, i) => result.samples[i] ?? null,
    );
    engine.loadSampler(sequenceFromSong(model), loadedSettings, sampleBytes);
    const haveStems = result.stems.some((s) => s && s.length > 0);
    if (haveStems) engine.loadStems(result.stems);
    const initialMode: PlaybackMode =
      loadedProject.samplerModeEnabled || !haveStems ? "sampler" : "chip";
    engine.setMode(initialMode);

    const names = loadedProject.sourceSamples.map((s) => s?.name ?? "");
    setBackend(engine);
    backendRef.current = engine;
    setSong(model);
    setProject(loadedProject);
    setSettings(loadedSettings);
    setSampleNames(names);
    setChannelVolume(loadedProject.channelVolume.slice(0, 4));
    setChannelMuted(loadedProject.mutedChannels.slice(0, 4));
    setMasterVolume(loadedProject.masterVolume);
    setMasterFx(loadedProject.masterFx);
    setReference(saved ? saved.reference : loadedProject.refPitchEnabled);
    setMode(initialMode);
    setStemsAvailable(haveStems);
    setCurrentFur(result.furBytes ?? null);
    setChipMix(result.chipMix ?? null);
    setEditor(null);
    setStatus(`${model.meta.name} — ${model.instruments.length} instruments`);
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const result = await loadDefaultSong();
        if (!cancelled) applyLoaded(result);
      } catch (e) {
        if (!cancelled) {
          setError(String(e));
          setStatus("");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [applyLoaded]);

  const changeMode = useCallback((next: PlaybackMode) => {
    backendRef.current?.setMode(next);
    setMode(next);
    setEditMode(false);
  }, []);

  const toggleEdit = useCallback(() => {
    setMode("sampler");
    backendRef.current?.setMode("sampler");
    setEditMode((prev) => !prev);
  }, []);

  const onPatternChanged = useCallback(() => {
    const model = songRef.current;
    if (model) backendRef.current?.updateSequence(sequenceFromSong(model));
  }, []);

  const onSeek = useCallback((time: number) => {
    backendRef.current?.seek(time);
  }, []);

  const onToggleChannel = useCallback((channel: number) => {
    setChannelMuted((prev) => {
      const next = prev.slice();
      next[channel] = !next[channel];
      backendRef.current?.setChannelMute(channel, next[channel]!);
      return next;
    });
  }, []);

  const onAudition = useCallback(
    (channels: number[], order: number, row: number) => {
      const model = songRef.current;
      const engine = backendRef.current;
      if (!model || !engine) return;
      const notes: Array<{ channel: number; instrument: number; rate: number; volume: number }> = [];
      for (const channel of channels) {
        const ch = model.channels[channel];
        if (!ch) continue;
        const note = ch.noteTimeline[order]?.[row];
        const instrument = ch.insTimeline[order]?.[row] ?? null;
        if (!note || instrument === null) continue;
        const setting = settingsRef.current[instrument];
        if (!setting || setting.muted || setting.sourceIndex === null) continue;
        if (setting.spectral.enabled && !engine.fusionReady(instrument)) continue;
        const rate = samplerPlaybackRate(note, model.meta.tuningA4, 0);
        if (rate === null || !(rate > 0)) continue;
        const cell = cellAt(model, channel, order, row);
        notes.push({
          channel,
          instrument,
          rate,
          volume: Math.min(cell.volume ?? 15, 15) / 15,
        });
      }
      if (notes.length === 0) return;
      engine.ensureStarted();
      engine.previewPattern(channels, rowTime(model, order, row), rowDuration(model, order * model.meta.patternLength + row) * 2, notes);
    },
    [],
  );

  const onChannelVolume = useCallback((channel: number, volume: number) => {
    backendRef.current?.setChannelVolume(channel, volume);
    setChannelVolume((prev) => {
      const next = prev.slice();
      next[channel] = volume;
      return next;
    });
  }, []);

  const onChannelMute = useCallback((channel: number, muted: boolean) => {
    backendRef.current?.setChannelMute(channel, muted);
    setChannelMuted((prev) => {
      const next = prev.slice();
      next[channel] = muted;
      return next;
    });
  }, []);

  const onMasterVolume = useCallback((volume: number) => {
    backendRef.current?.setMasterVolume(volume);
    setMasterVolume(volume);
  }, []);

  const onUpdateSetting = useCallback((index: number, patch: Partial<SamplerSettings>) => {
    const engine = backendRef.current;
    setSettings((prev) => {
      const next = prev.slice();
      const current = next[index] ?? defaultSamplerSettings();
      let merged: SamplerSettings = { ...current, ...patch };
      // Source changes reset the trim only while driving the plain sampler.
      if (patch.sourceIndex !== undefined && !merged.spectral.enabled) {
        const duration = engine?.sampleDurations()[patch.sourceIndex ?? -1] ?? 0;
        merged = { ...merged, startSec: 0, endSec: duration };
      }
      engine?.setSamplerSettings(index, merged);
      // Spectral parameter/source changes rebuild the render, then re-trim to it.
      if (merged.spectral.enabled && (patch.spectral !== undefined || patch.sourceIndex !== undefined)) {
        engine?.renderFusion(index);
        const duration = engine?.effectiveDuration(index) ?? 0;
        if (duration > 0) {
          merged = { ...merged, startSec: 0, endSec: duration };
          engine?.setSamplerSettings(index, merged);
        }
      }
      next[index] = merged;
      return next;
    });
  }, []);

  const applyEngine = useCallback((index: number, spectral: boolean) => {
    const engine = backendRef.current;
    setSettings((prev) => {
      const next = prev.slice();
      const current = next[index] ?? defaultSamplerSettings();
      const merged: SamplerSettings = { ...current, spectral: { ...current.spectral } };
      setSpectralEnabled(merged, spectral);
      engine?.setSamplerSettings(index, merged);
      if (spectral) {
        engine?.renderFusion(index);
        const duration = engine?.effectiveDuration(index) ?? 0;
        if (duration > 0) {
          merged.startSec = 0;
          merged.endSec = duration;
          engine?.setSamplerSettings(index, merged);
        }
      }
      next[index] = merged;
      return next;
    });
  }, []);

  const onTimingEdit = useCallback((patch: TimingEditPatch) => {
    const model = songRef.current;
    if (!model) return;
    if (patch.tickRate != null) model.meta.tickRate = patch.tickRate;
    if (patch.speed != null) {
      if (model.meta.speedPattern.length > 0) model.meta.speedPattern[0] = patch.speed;
      else model.meta.speedPattern.push(patch.speed);
    }
    if (patch.highlightA != null) model.meta.highlightA = patch.highlightA;
    if (patch.highlightB != null) model.meta.highlightB = patch.highlightB;
    if (patch.virtualTempo) model.meta.virtualTempo = patch.virtualTempo;
    retime(model);
    backendRef.current?.updateSequence(sequenceFromSong(model));
    // Nudge React so the controlled timing inputs re-render.
    setSettings((prev) => prev.slice());
  }, []);

  const onTransposeAdjust = useCallback((index: number, delta: number) => {
    const engine = backendRef.current;
    const current = settingsRef.current[index] ?? defaultSamplerSettings();
    const transpose = Math.min(Math.max(current.transpose + delta, -48), 48);
    const merged: SamplerSettings = { ...current, transpose };
    const next = settingsRef.current.slice();
    next[index] = merged;
    settingsRef.current = next;
    setSettings(next);
    engine?.setSamplerSettings(index, merged);
    engine?.preview(index, referenceRef.current);
  }, []);

  const onUpdateInstrument = useCallback((index: number, patch: Partial<InstrumentInfo>) => {
    const model = songRef.current;
    if (!model) return;
    const info = model.instruments[index];
    if (!info) return;
    Object.assign(info, patch);
    setSettings((prev) => prev.slice());
  }, []);

  const onMetadataEdit = useCallback((patch: Partial<ProjectFile>) => {
    setProject((prev) => (prev ? { ...prev, ...patch } : prev));
  }, []);

  const newProject = useCallback(() => {
    const model = songRef.current;
    const engine = backendRef.current;
    if (!model || !engine) return;
    engine.stop();
    const snapshot = patternSnapshot(model);
    clearPatternsSnapshot(snapshot, model.meta.patternLength);
    applySnapshot(model, snapshot);
    model.meta.comment = "";
    if (model.instruments.length > 1) model.instruments = model.instruments.slice(0, 1);
    if (model.instruments[0]) {
      model.instruments[0].name = "Instrument 01";
      model.instruments[0].colorRgb = instrumentColor(0);
    }
    const nextSettings = [defaultSamplerSettings()];
    settingsRef.current = nextSettings;
    setSettings(nextSettings);
    engine.setSamplerSettings(0, nextSettings[0]!);
    engine.updateSequence(sequenceFromSong(model));
    // Reset all volume levels to defaults.
    for (let c = 0; c < 4; c++) {
      engine.setChannelVolume(c, 1);
      engine.setChannelMute(c, false);
    }
    engine.setMasterVolume(1);
    setChannelVolume([1, 1, 1, 1]);
    setChannelMuted([false, false, false, false]);
    setMasterVolume(1);
    setEditor(null);
    setStemsAvailable(false);
    engine.setMasterFx(defaultMasterFx());
    setMasterFx(defaultMasterFx());
    setProject((prev) => {
      const base = prev ?? defaultProject();
      return {
        ...base,
        samplerModeEnabled: mode === "sampler",
        songTitle: NEW_PROJECT_TITLE,
        artist: NEW_PROJECT_ARTIST,
        album: NEW_PROJECT_ALBUM,
        comments: "",
        channelVolume: [1, 1, 1, 1],
        masterVolume: 1,
        mutedChannels: [false, false, false, false],
        mutedInstruments: [false],
        instruments: nextSettings.map((setting) => ({ ...setting, muted: false })),
        masterFx: defaultMasterFx(),
        patternSnapshot: patternSnapshot(model),
        tickRateOverride: model.meta.tickRate,
        speedOverride: model.meta.speedPattern[0] ?? null,
        highlightAOverride: model.meta.highlightA,
        highlightBOverride: model.meta.highlightB,
        virtualTempoOverride: model.meta.virtualTempo,
      };
    });
    setStatus("New project");
  }, [mode]);

  const addInstrument = useCallback(() => {
    const model = songRef.current;
    const engine = backendRef.current;
    if (!model) return;
    const index = model.instruments.length;
    model.instruments.push({
      name: `Instrument ${(index + 1).toString().padStart(2, "0")}`,
      insType: 2,
      gameBoy: null,
      colorRgb: instrumentColor(index),
    });
    const blank = defaultSamplerSettings();
    const next = settingsRef.current.slice();
    next.push(blank);
    settingsRef.current = next;
    setSettings(next);
    engine?.setSamplerSettings(index, blank);
    setProject((prev) =>
      prev ? { ...prev, mutedInstruments: [...prev.mutedInstruments, false] } : prev,
    );
    setStatus(`Added instrument ${(index + 1).toString().padStart(2, "0")}`);
  }, []);

  const deleteInstrument = useCallback(
    (index: number, mode: "remove" | "reassign", target: number) => {
    const model = songRef.current;
    const engine = backendRef.current;
    if (!model || !engine) return;
    if (model.instruments.length <= 1) {
      setStatus("Cannot delete the last instrument");
      return;
    }
    const current = settingsRef.current[index];
    if (current) engine.setSamplerSettings(index, { ...current, muted: true });
    const snapshot = patternSnapshot(model);
    if (mode === "reassign") reassignInstrument(snapshot, index, target);
    else remapInstrumentsAfterDelete(snapshot, index);
    model.instruments.splice(index, 1);
    applySnapshot(model, snapshot);
    const nextSettings = settingsRef.current.slice();
    nextSettings.splice(index, 1);
    settingsRef.current = nextSettings;
    setSettings(nextSettings);
    engine.replaceSettings(nextSettings);
    nextSettings.forEach((setting, i) => engine.setSamplerSettings(i, setting));
    engine.updateSequence(sequenceFromSong(model));
    setProject((prev) => {
      if (!prev) return prev;
      const muted = prev.mutedInstruments.slice();
      muted.splice(index, 1);
      return { ...prev, mutedInstruments: muted };
    });
    setEditor((prev) => {
      if (!prev) return prev;
      if (prev.index === index) return null;
      if (prev.index > index) return { ...prev, index: prev.index - 1 };
      return prev;
    });
    setStatus(`Deleted instrument ${index.toString().padStart(2, "0")}`);
    },
    [],
  );

  const instrumentMuted = useMemo(
    () => settings.map((s) => s.muted),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [settings.map((s) => s.muted).join(",")],
  );

  const onMasterFxChange = useCallback((settings: MasterFxSettings) => {
    setMasterFx(settings);
    backendRef.current?.setMasterFx(settings);
  }, []);

  const onOpenMasterFx = useCallback(() => setMasterFxOpen(true), []);

  const onViewOrderChange = useCallback((order: number) => {
    viewOrderRef.current = order;
  }, []);

  const onSelectionChange = useCallback(
    (selection: { order: number; row: number } | null) => {
      selectionRef.current = selection;
    },
    [],
  );

  const onPreview = useCallback(
    (index: number) => {
      const engine = backendRef.current;
      if (!engine) return;
      engine.ensureStarted();
      if (mode === "chip") {
        const model = songRef.current;
        const spot = model ? findInstrumentSpot(model, index) : null;
        if (model && spot) {
          engine.previewPattern(
            [spot.channel],
            rowTime(model, spot.order, spot.row),
            rowDuration(model, spot.order * model.meta.patternLength + spot.row) * 2,
            [],
          );
          return;
        }
      }
      engine.preview(index, referenceRef.current);
    },
    [mode],
  );

  const loadSample = useCallback(
    async (slot: number) => {
      const choice = await chooseAudioFile();
      if ("error" in choice) {
        if (choice.error !== "cancelled") setError(choice.error);
        return;
      }
      backendRef.current?.replaceSample(slot, choice.bytes);
      setSampleNames((prev) => {
        const next = prev.slice();
        while (next.length < 6) next.push("");
        next[slot] = choice.name;
        return next;
      });
      setProject((prev) => {
        if (!prev) return prev;
        const sourceSamples = prev.sourceSamples.slice();
        while (sourceSamples.length < 6) sourceSamples.push(null);
        sourceSamples[slot] = { name: choice.name, url: null, comments: "", dataUrl: null };
        return { ...prev, sourceSamples };
      });
      setStatus(`Loaded Source Sample slot ${slot}`);
    },
    [],
  );

  const onSampleLoad = useCallback((slot: number) => void loadSample(slot), [loadSample]);
  const onSamplePlay = useCallback((slot: number) => backendRef.current?.previewSample(slot), []);
  const onSampleStop = useCallback(() => backendRef.current?.stopSamplePreview(), []);
  const onSampleInfo = useCallback(
    (slot: number) => {
      setSampleInfo(slot);
      setSampleInfoName(sampleNames[slot] ?? "");
      setSampleInfoComments(project?.sourceSamples[slot]?.comments ?? "");
    },
    [sampleNames, project],
  );
  const onSampleClear = useCallback(() => setClearConfirm(true), []);

  const clearSamples = useCallback(() => {
    const engine = backendRef.current;
    if (!engine) return;
    engine.stopSamplePreview();
    for (let slot = 0; slot < 6; slot++) engine.replaceSample(slot, null);
    setSampleNames(Array.from({ length: 6 }, () => ""));
    setProject((prev) => (prev ? { ...prev, sourceSamples: Array.from({ length: 6 }, () => null) } : prev));
    setSettings((prev) =>
      prev.map((s) => {
        const defaults = defaultSamplerSettings();
        return {
          ...s,
          sourceIndex: null,
          startSec: 0,
          endSec: 0,
          attack: defaults.attack,
          decay: defaults.decay,
          sustain: defaults.sustain,
          release: defaults.release,
        };
      }),
    );
    setClearConfirm(false);
    setStatus("Cleared source samples");
  }, []);

  const openProjectJson = useCallback(() => {
    if (!song || !project) return;
    // Open immediately; the (occasionally large) serialization runs on the next
    // tick so the modal paints without waiting for it.
    setProjectIoOpen(true);
    setProjectIoText("");
    const model = song;
    const baseProject = project;
    window.setTimeout(() => {
      const live: ProjectFile = {
        ...baseProject,
        samplerModeEnabled: mode === "sampler",
        channelVolume,
        masterVolume,
        mutedChannels: channelMuted,
        mutedInstruments: settings.map((s) => s.muted),
        refPitchEnabled: reference,
        theme,
        instruments: settings.map((s) => ({ ...s, muted: false })),
        instrumentNames: model.instruments.map((i) => i.name),
        masterFx,
        patternSnapshot: patternSnapshot(model),
        tickRateOverride: model.meta.tickRate,
        speedOverride: model.meta.speedPattern[0] ?? null,
        highlightAOverride: model.meta.highlightA,
        highlightBOverride: model.meta.highlightB,
        virtualTempoOverride: model.meta.virtualTempo,
      };
      setProjectIoText(projectToJson(live, true));
    }, 0);
  }, [song, project, mode, channelVolume, masterVolume, channelMuted, settings, reference, theme, masterFx]);

  const applyProjectText = useCallback(
    (text: string) => {
    if (!song) return;
    try {
      const parsed = projectFromJson(text);
      // Adopt the JSON's instrument count (grow with blanks / shrink safely).
      const targetCount = parsed.instruments.length;
      if (targetCount > 0) {
        if (song.instruments.length > targetCount) {
          const snapshot = patternSnapshot(song);
          for (const channel of snapshot.channels) {
            for (const [, rows] of channel.patterns) {
              for (const cell of rows) {
                if (cell.instrument !== null && cell.instrument >= targetCount) {
                  cell.instrument = null;
                }
              }
            }
          }
          song.instruments = song.instruments.slice(0, targetCount);
          applySnapshot(song, snapshot);
        } else {
          while (song.instruments.length < targetCount) {
            const idx = song.instruments.length;
            song.instruments.push({
              name: `Instrument ${(idx + 1).toString().padStart(2, "0")}`,
              insType: 2,
              gameBoy: null,
              colorRgb: instrumentColor(idx),
            });
          }
        }
      }
      validateProject(parsed, song.instruments.length);
      parsed.instrumentNames.forEach((name, i) => {
        if (song.instruments[i]) song.instruments[i]!.name = name;
      });
      if (parsed.patternSnapshot) applySnapshot(song, parsed.patternSnapshot);
      applyTimingOverrides(parsed, song);
      const engine = backendRef.current;
      for (let c = 0; c < 4; c++) {
        engine?.setChannelVolume(c, parsed.channelVolume[c] ?? 1);
        engine?.setChannelMute(c, parsed.mutedChannels[c] ?? false);
      }
      engine?.setMasterVolume(parsed.masterVolume);
      engine?.setMasterFx(parsed.masterFx);
      const nextSettings = song.instruments.map(
        (_, i) => parsed.instruments[i] ?? defaultSamplerSettings(),
      );
      parsed.mutedInstruments.forEach((muted, i) => {
        if (nextSettings[i]) nextSettings[i]!.muted = !!muted;
      });
      nextSettings.forEach((s, i) => engine?.setSamplerSettings(i, s));
      // Reconstruct Spectral renders for enabled instruments, preserving saved trim.
      nextSettings.forEach((s, i) => {
        if (s.spectral.enabled) engine?.renderFusion(i);
      });
      engine?.updateSequence(sequenceFromSong(song));
      setSettings(nextSettings);
      setProject(parsed);
      setSampleNames(parsed.sourceSamples.map((s) => s?.name ?? ""));
      setChannelVolume(parsed.channelVolume.slice(0, 4));
      setChannelMuted(parsed.mutedChannels.slice(0, 4));
      setMasterVolume(parsed.masterVolume);
      setMasterFx(parsed.masterFx);
      setReference(parsed.refPitchEnabled);
      setProjectIoOpen(false);
      setStatus("Applied Project JSON");
    } catch (e) {
      setError(String(e));
    }
    },
    [song],
  );

  const loadProjectFile = useCallback(
    (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (!file) return;
      if (!file.name.toLowerCase().endsWith(".lampjson")) {
        setError(`Unsupported project file "${file.name}" — expected a .lampjson file.`);
        return;
      }
      void file.text().then((text) => {
        setProjectIoText(text);
        applyProjectText(text);
        setStatus(`Loaded ${file.name}`);
      });
    },
    [applyProjectText],
  );

  const saveBytes = useCallback(async (name: string, bytes: Uint8Array) => {
    const ok = await saveFile(name, bytes);
    if (ok) setStatus(`Saved ${name}`);
  }, []);

  const saveFur = useCallback(() => {
    if (currentFur && song) void saveBytes(`${safeFilename(song.meta.name)}.fur`, currentFur);
  }, [currentFur, song, saveBytes]);

  const saveMidi = useCallback(() => {
    if (song) void saveBytes(`${safeFilename(song.meta.name)}.mid`, writeMidi(song));
  }, [song, saveBytes]);

  const saveWav = useCallback(() => {
    if (!song) return;
    setWavExportOpen(true);
  }, [song]);

  const runWavExport = useCallback(
    async (params: ExportParams) => {
      if (!song) return;
      setWavExportBusy(true);
      setWavExportProgress(0);
      try {
        // Yield so the progress modal can paint before the synchronous mixdown.
        await new Promise((resolve) => setTimeout(resolve, 30));
        const title = project?.songTitle || song.meta.name;
        const isChip = mode === "chip" && chipMix !== null;
        setWavExportProgress(0.08);
        let base: AudioClip;
        if (isChip && chipMix) {
          base = await decodeAudioBytes(chipMix);
        } else {
          const engine = backendRef.current;
          if (!engine) return;
          const clips = settings.map((_, i) => engine.effectiveClip(i));
          base = renderSamplerMix(
            sequenceFromSong(song),
            settings,
            clips,
            channelVolume,
            channelMuted,
            masterVolume,
          );
        }
        setWavExportProgress(0.2);

        // Arrange all loops + fade tail first, then run the Master FX chain once
        // over the continuous signal so reverb/delay tails overlap the next loop
        // instead of leaving a gap between them. With a fade we trim back to the
        // arrangement so the file ends exactly at the fade; without one we let
        // the final tail ring out.
        const arranged = arrangeExport(base, params.loops, params.fadeOutMs);
        const wet = await applyMasterFxOffline(arranged, masterFx, (fraction) =>
          setWavExportProgress(0.2 + fraction * 0.7),
        );
        const trimmed = params.fadeOutMs > 0 ? clipSlice(wet, clipLen(arranged)) : wet;
        const final = applyExportEnvelope(trimmed, params);
        setWavExportProgress(0.93);

        const artwork = (await coverRef.current?.renderPngBytes()) ?? undefined;
        const bytes = wavPcm16(final, {
          title,
          artist: project?.artist || undefined,
          album: project?.album || undefined,
          artwork,
        });
        setWavExportProgress(0.97);

        const now = new Date();
        const stamp = `${String(now.getFullYear() % 100).padStart(2, "0")}${String(
          now.getMonth() + 1,
        ).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`;
        const name = `${stamp}- ${safeFilename(title)}.wav`;
        await saveBytes(name, bytes);
        setWavExportProgress(1);
        setWavExportOpen(false);
      } catch (e) {
        setError(`WAV export failed: ${String(e)}`);
        setWavExportOpen(false);
      } finally {
        setWavExportBusy(false);
      }
    },
    [
      song,
      mode,
      chipMix,
      settings,
      channelVolume,
      channelMuted,
      masterVolume,
      project,
      masterFx,
      saveBytes,
    ],
  );

  const packageSamples = useCallback(() => {
    const engine = backendRef.current;
    if (!engine) return;
    const entries: Array<{ name: string; data: Uint8Array }> = [];
    for (let slot = 0; slot < 6; slot++) {
      if (!project?.sourceSamples[slot]) continue;
      const clip = engine.sampleClip(slot);
      if (clip) entries.push({ name: `${slot}.wav`, data: wavPcm16(clip) });
    }
    if (entries.length === 0) {
      setError("No decoded Source Samples are available to package");
      return;
    }
    const title = project ? project.songTitle : "source-samples";
    void saveBytes(`${safeFilename(title)}-source-samples.zip`, zipStore(entries));
  }, [project, song, saveBytes]);

  const openEditor = useCallback(
    (index: number, spectral: boolean) => {
      applyEngine(index, spectral);
      setEditor({ index, spectral });
    },
    [applyEngine],
  );

  const infoProject = project;

  return (
    <ExplainerContext.Provider value={setExplainer}>
    <div className="app" data-mode={editMode ? "edit" : mode}>
      <header className="app-header">
        <div>
          <h1>Lantern Music Player {BUILD_NUMBER}</h1>
          {song && (
            <p className="subtitle">
              {project?.songTitle || song.meta.name} — {project?.artist || song.meta.author}
            </p>
          )}
        </div>
        <div className="header-right">
          <button
            className="theme-toggle"
            onClick={() => setTheme((t) => (t === "light" ? "dark" : "light"))}
            title="Toggle Light / Dark theme"
          >
            {theme === "light" ? "☀ Light" : "☾ Dark"}
          </button>
        </div>
      </header>

      {error && (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button className="error-dismiss" onClick={() => setError(null)} title="Dismiss error">
            ✕
          </button>
        </div>
      )}

      {song && backend && (
        <main className="layout">
          <div className="main-column">
            <Toolbar
              onNewProject={() => setNewProjectConfirm(true)}
              onSaveFur={saveFur}
              onSaveMidi={saveMidi}
              onOpenProjectJson={openProjectJson}
              onSaveWav={saveWav}
              wavReady={mode === "sampler" || chipMix !== null}
              furReady={currentFur !== null}
              status={status}
            />
            <SongMetaCard
              project={project}
              song={song}
              editMode={editMode}
              mode={mode}
              anySpectral={settings.some((s) => s.spectral.enabled)}
              onEdit={onMetadataEdit}
            />
            <Transport
              backend={backend}
              song={song}
              mode={mode}
              stemsAvailable={stemsAvailable}
              editMode={editMode}
              onModeChange={changeMode}
              onToggleEdit={toggleEdit}
            />
            <AudioError backend={backend} />
            <PatternGrid
              song={song}
              backend={backend}
              editMode={editMode}
              channelMuted={channelMuted}
              instrumentMuted={instrumentMuted}
              showChannelTypes={mode === "chip" && !editMode}
              onChanged={onPatternChanged}
              onSeek={onSeek}
              onToggleChannel={onToggleChannel}
              onAudition={onAudition}
              onViewOrderChange={onViewOrderChange}
              onSelectionChange={onSelectionChange}
            />
            <InstrumentList
              backend={backend}
              song={song}
              settings={settings}
              sampleNames={sampleNames}
              onUpdate={onUpdateSetting}
              onUpdateInstrument={onUpdateInstrument}
              onTranspose={onTransposeAdjust}
              onPreview={onPreview}
              onOpenEditor={openEditor}
              onAddInstrument={addInstrument}
              onRequestDelete={(index) =>
                setDeletePrompt({ index, mode: "remove", target: index === 0 ? 1 : 0 })
              }
            />
            <Piano song={song} backend={backend} />
            <SourceSamples
              backend={backend}
              song={song}
              project={project}
              sampleNames={sampleNames}
              onLoad={onSampleLoad}
              onPlay={onSamplePlay}
              onStop={onSampleStop}
              onInfo={onSampleInfo}
              onClear={onSampleClear}
              onPackage={packageSamples}
            />
          </div>
          <aside className="sidebar">
            <CoverArt ref={coverRef} song={song} backend={backend} title={project?.songTitle || song.meta.name} />
            <ExplainerCard content={explainer} />
            <SongComments
              comments={project?.comments ?? ""}
              fallback={song.meta.comment}
              editMode={editMode}
              onChange={(value) =>
                setProject((prev) => (prev ? { ...prev, comments: value } : prev))
              }
            />
            <TimingCard song={song} editMode={editMode} onEdit={onTimingEdit} />
            <Mixer
              backend={backend}
              channelVolume={channelVolume}
              channelMuted={channelMuted}
              masterVolume={masterVolume}
              onChannelVolume={onChannelVolume}
              onChannelMute={onChannelMute}
              onMasterVolume={onMasterVolume}
              onOpenMasterFx={onOpenMasterFx}
            />
            <LicensesCard project={project} />
          </aside>
        </main>
      )}

      {editor && song && settings[editor.index] && (
        <SamplerEditor
          backend={backend!}
          index={editor.index}
          name={song.instruments[editor.index]?.name ?? `Instrument ${editor.index}`}
          color={song.instruments[editor.index]?.colorRgb ?? [201, 151, 58]}
          settings={settings[editor.index]!}
          sampleNames={sampleNames}
          reference={reference}
          onReferenceChange={setReference}
          spectralTab={editor.spectral}
          wasmAvailable={wasmReady}
          onTabChange={(spectral) => {
            applyEngine(editor.index, spectral);
            setEditor({ ...editor, spectral });
          }}
          onUpdate={(patch) => onUpdateSetting(editor.index, patch)}
          onClose={() => setEditor(null)}
        />
      )}

      {projectIoOpen && (
        <DraggableModal title="Project JSON" onClose={() => setProjectIoOpen(false)}>
          <textarea
            className="project-json"
            value={projectIoText}
            onChange={(e) => setProjectIoText(e.target.value)}
            spellCheck={false}
          />
          <div className="row">
            <button
              onClick={() => jsonFileInputRef.current?.click()}
              disabled={typeof File === "undefined"}
              title={
                typeof File === "undefined"
                  ? "File loading is not supported in this browser"
                  : "Load a Lantern Project file (.lampjson) from disk"
              }
            >
              Load…
            </button>
            <input
              ref={jsonFileInputRef}
              type="file"
              accept=".lampjson"
              style={{ display: "none" }}
              onChange={loadProjectFile}
            />
            <button onClick={() => void navigator.clipboard.writeText(projectIoText)}>Copy</button>
            <button onClick={() => applyProjectText(projectIoText)}>Apply</button>
            <button
              onClick={() =>
                void saveBytes(
                  `${safeFilename(project ? project.songTitle : "project")}.lampjson`,
                  new TextEncoder().encode(projectIoText),
                )
              }
            >
              Download
            </button>
          </div>
        </DraggableModal>
      )}

      {sampleInfo !== null && (
        <DraggableModal
          title={`Source Sample ${sampleInfo}`}
          onClose={() => setSampleInfo(null)}
        >
          <label>Name</label>
          <input value={sampleInfoName} onChange={(e) => setSampleInfoName(e.target.value)} />
          <label>Comments</label>
          <textarea
            value={sampleInfoComments}
            onChange={(e) => setSampleInfoComments(e.target.value)}
          />
          <div className="row">
            <button
              onClick={() => {
                setSampleNames((prev) => {
                  const next = prev.slice();
                  while (next.length < 6) next.push("");
                  next[sampleInfo] = sampleInfoName;
                  return next;
                });
                setProject((prev) => {
                  if (!prev) return prev;
                  const sourceSamples = prev.sourceSamples.slice();
                  while (sourceSamples.length < 6) sourceSamples.push(null);
                  const existing = sourceSamples[sampleInfo];
                  sourceSamples[sampleInfo] = {
                    name: sampleInfoName,
                    url: existing?.url ?? null,
                    comments: sampleInfoComments,
                    dataUrl: existing?.dataUrl ?? null,
                  };
                  return { ...prev, sourceSamples };
                });
                setSampleInfo(null);
              }}
            >
              Save
            </button>
          </div>
        </DraggableModal>
      )}

      {song && deletePrompt && (
        <DraggableModal
          title={`Delete Instrument ${deletePrompt.index.toString().padStart(2, "0")}?`}
          onClose={() => setDeletePrompt(null)}
        >
          <p>What should happen to the pattern notes that use this instrument?</p>
          <label className="radio-row">
            <input
              type="radio"
              checked={deletePrompt.mode === "remove"}
              onChange={() => setDeletePrompt({ ...deletePrompt, mode: "remove" })}
            />
            Remove all notes that use it
          </label>
          <label className="radio-row">
            <input
              type="radio"
              checked={deletePrompt.mode === "reassign"}
              onChange={() => setDeletePrompt({ ...deletePrompt, mode: "reassign" })}
            />
            Re-assign those notes to
          </label>
          <select
            disabled={deletePrompt.mode !== "reassign"}
            value={deletePrompt.target}
            onChange={(e) =>
              setDeletePrompt({ ...deletePrompt, mode: "reassign", target: Number(e.target.value) })
            }
          >
            {song.instruments.map((ins, idx) =>
              idx === deletePrompt.index ? null : (
                <option key={idx} value={idx}>
                  {idx.toString(16).toUpperCase().padStart(2, "0")}: {ins.name || `Instrument ${idx}`}
                </option>
              ),
            )}
          </select>
          <p className="hint">
            The instrument is removed and later instruments shift down one index. This cannot be
            undone.
          </p>
          <div className="row">
            <button
              onClick={() => {
                deleteInstrument(deletePrompt.index, deletePrompt.mode, deletePrompt.target);
                setDeletePrompt(null);
              }}
            >
              Delete Instrument
            </button>
            <button onClick={() => setDeletePrompt(null)}>Cancel</button>
          </div>
        </DraggableModal>
      )}

      {masterFxOpen && (
        <MasterFxModal
          settings={masterFx}
          onChange={onMasterFxChange}
          onClose={() => setMasterFxOpen(false)}
        />
      )}

      {wavExportOpen && (
        <WavExportModal
          busy={wavExportBusy}
          progress={wavExportProgress}
          onExport={runWavExport}
          onClose={() => setWavExportOpen(false)}
        />
      )}

      {newProjectConfirm && (
        <DraggableModal title="New Project?" onClose={() => setNewProjectConfirm(false)}>
          <p>
            Create a new project? This clears all patterns, resets every instrument to its default
            settings, deletes all but the first instrument, and resets volume levels.
          </p>
          <div className="row">
            <button
              onClick={() => {
                newProject();
                setNewProjectConfirm(false);
              }}
            >
              Create New Project
            </button>
            <button onClick={() => setNewProjectConfirm(false)}>Cancel</button>
          </div>
        </DraggableModal>
      )}

      {clearConfirm && (
        <DraggableModal title="Clear Source Samples?" onClose={() => setClearConfirm(false)}>
          <p>
            Remove all six loaded source samples from this session? Files on disk are not deleted.
          </p>
          <div className="row">
            <button onClick={clearSamples}>Clear Samples</button>
            <button onClick={() => setClearConfirm(false)}>Cancel</button>
          </div>
        </DraggableModal>
      )}

    </div>
    </ExplainerContext.Provider>
  );
}
