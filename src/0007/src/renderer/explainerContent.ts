import type { PatternCell } from "@/core/fur/types";
import { noteToFreq, noteToName } from "@/core/pitch";
import type { SongModel } from "@/core/songModel";
import { FX_CATALOG, columnLabel, type EditColumn } from "@/core/tracker";

export interface ExplainerText {
  title: string;
  body: string;
}

const CHANNEL_ROLES = [
  "Pulse 1 — a square wave with a selectable duty cycle and a Game Boy hardware envelope.",
  "Pulse 2 — the same hardware as Pulse 1, used as a second voice (no frequency sweep).",
  "Wave — plays a custom 32-sample, 4-bit waveform instead of a fixed shape.",
  "Noise — a pseudo-random LFSR generator, typically used for percussion.",
];

const CHANNEL_NAMES = ["Pulse 1", "Pulse 2", "Wave", "Noise"];

function hex2(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, "0");
}

function instrumentDescription(song: SongModel, index: number): string {
  const ins = song.instruments[index];
  if (!ins) return `instrument ${index}`;
  const gb = ins.gameBoy;
  if (!gb) return ins.name || `#${index}`;
  return `${ins.name || `#${index}`} (envelope ${gb.envelopeVolume} ${
    gb.envelopeDirection ? "up" : "down"
  }/${gb.envelopeLength}${gb.softwareEnvelope ? ", software envelope" : ""})`;
}

export function songExplain(song: SongModel): ExplainerText {
  return {
    title: "Song — top-level metadata",
    body: `From the Furnace module's song info — name, author, system and tuning (${song.meta.tuningA4} Hz = A4).\n\nThe tracker data below is parsed from the binary .fur file, not a pre-baked blob.`,
  };
}

export function commentsExplain(): ExplainerText {
  return {
    title: "Song Comments — from the composer",
    body: "The composer's own notes, verbatim — recording notes, licensing, credits, anything worth knowing before the data below. Editable in EDIT MODE and saved with the project.",
  };
}

export function timingExplain(song: SongModel): ExplainerText {
  const speed = song.meta.speedPattern[0] ?? 6;
  const rowMs = (speed / Math.max(song.meta.tickRate, 1)) * 1000;
  return {
    title: "Timing — how fast the tracker ticks",
    body: `rowDuration = speed / tickRate.\n\nTick rate ${song.meta.tickRate.toFixed(
      3,
    )} Hz, speed ${speed} ticks/row → ${rowMs.toFixed(
      1,
    )} ms/row. Virtual tempo ${song.meta.virtualTempo[0]}/${song.meta.virtualTempo[1]} fine-tunes it further.\n\nPattern count ${song.meta.orderLength} × pattern length ${song.meta.patternLength} rows = the full arrangement per channel.`,
  };
}

export function chipsExplain(song: SongModel): ExplainerText {
  const names = song.chips.map((c) => (c.chipId === 4 ? "Game Boy" : `Chip ${c.chipId}`));
  return {
    title: "Chips — the sound hardware being emulated",
    body: `${names.join(", ")} with ${song.channels.length} channels. Volume/panning are master-bus trims, separate from per-instrument and per-row volume.`,
  };
}

export function instrumentsExplain(song: SongModel): ExplainerText {
  return {
    title: "Instruments — Game Boy channel presets",
    body: `${song.instruments.length} instruments — a Game Boy envelope plus optional duty-cycle/wavetable settings, triggered by index. Hover a row for its envelope; use the Sampler/Spectral pills to edit, or the hamburger to delete.`,
  };
}

export function instrumentExplain(song: SongModel, index: number): ExplainerText {
  const ins = song.instruments[index];
  const gb = ins?.gameBoy;
  return {
    title: `Instrument ${index.toString().padStart(2, "0")} · ${ins?.name ?? ""}`,
    body: gb
      ? `Game Boy envelope: starts at volume ${gb.envelopeVolume}, ${
          gb.envelopeDirection ? "increasing" : "decreasing"
        } every ${gb.envelopeLength} step(s). Sound length ${gb.soundLength} (64 = held). Software envelope: ${
          gb.softwareEnvelope ? "yes" : "no"
        }.\n\nThe colour follows held notes through the tracker, piano and cover scans; its quick controls stay in sync with the movable Sampler/Spectral window.`
      : "A Furnace instrument. Use the Sampler/Spectral pills to edit it; its colour follows held notes through the tracker, piano and cover scans.",
  };
}

export function transposeExplain(): ExplainerText {
  return {
    title: "Transpose — pitch-shift this instrument's sample",
    body: "−/+ step one semitone at a time and preview immediately, matching the Transpose field in this instrument's own Sampler window. With no source sample assigned it is still adjustable, just silent.",
  };
}

export function instrumentVolumeExplain(): ExplainerText {
  return {
    title: "Volume — per-instrument level",
    body: "Sits on top of the pattern's own volume column, in dB, capped at 0 dB. Same field as this instrument's Sampler window, editable from either place.",
  };
}

export function mixerExplain(): ExplainerText {
  return {
    title: "Mixer — levels and clipping",
    body: "Per-channel plus master volume, each with a live peak meter and an editable dB field that tracks the slider (type a value, Enter/blur to apply).\n\nThe meter turns red near full scale — a warning worth heeding before exporting a WAV.",
  };
}

export function mixerChannelExplain(channel: number): ExplainerText {
  return {
    title: `CH${channel} volume · ${CHANNEL_NAMES[channel]}`,
    body: "Scales this channel before the master bus, independent of the mute toggle on the Patterns header (both multiply).",
  };
}

export function mixerMasterExplain(): ExplainerText {
  return {
    title: "Master — the final output stage",
    body: "Every channel sums here before output/export. If the master clips even with channels under 100%, turn individual channels down rather than the master.",
  };
}

export function sourceSamplesExplain(): ExplainerText {
  return {
    title: "Source Samples — Sampler Mode's raw material",
    body: "Up to six clips. There is no manifest — slot i is just ASSETS/SourceSamples/i.ogg (or .wav); swapping that exact file fills it.\n\nClick ▶ to preview. An instrument points at a slot via its own Sampler button to play it, pitched to the note.",
  };
}

export function packageSamplesExplain(): ExplainerText {
  return {
    title: "Package Samples — save loaded samples as files",
    body: "Downloads every populated slot as one .zip of numbered .wav files, matching the i.ogg / i.wav naming, so unzipping straight into ASSETS/SourceSamples/ just works.\n\nMainly for anything brought in via a slot's own Load button — that only lasts this session otherwise.",
  };
}

export function sampleLoadExplain(): ExplainerText {
  return {
    title: "Load — bring in a file from your computer",
    body: "Decodes a .wav/.mp3/.ogg from your computer straight into this slot, overwriting whatever was there. Session only — use Package Samples once you want to keep it.",
  };
}

export function clearSamplesExplain(): ExplainerText {
  return {
    title: "Clear Samples — wipe every slot",
    body: "Empties all six slots and resets every instrument's Start/End and ADSR back to defaults, since they were tuned against audio that's now gone. Files on disk aren't touched.",
  };
}

export function samplePreviewExplain(): ExplainerText {
  return {
    title: "Preview — hear the raw Source Sample",
    body: "Plays this clip in full at its original pitch/speed, independent of any instrument's settings. Click again (■) to stop early.",
  };
}

export function sampleInfoExplain(): ExplainerText {
  return {
    title: "Info — name and notes for this sample",
    body: "Editable name (shown as its title here and in the Sampler window's Source list) plus free-text notes — origin, licensing, usage. Both are saved and included in Project JSON.",
  };
}

export function samplerModeExplain(mode: "chip" | "sampler"): ExplainerText {
  return mode === "sampler"
    ? {
        title: "Sampler Mode — which engine is actually playing",
        body: "SAMPLER MODE (green): assigned instruments play via the sample engine — pitch-shifted, ADSR-shaped, per-instrument poly/mono.",
      }
    : {
        title: "Chip Mode — which engine is actually playing",
        body: "CHIP MODE (blue): plays the four original Furnace-rendered stems. It also changes what Save .WAV exports and the Song summary line.",
      };
}

export function spectralFusionExplain(): ExplainerText {
  return {
    title: "Spectral Fusion — morph two samples into one",
    body: "Pick a mode (and Sample B, if it needs one) — this instrument then plays the fused result everywhere (tracker, preview, export) instead of its plain sample. It re-renders automatically whenever a parameter changes.\n\nStart/End resets to the fused sample; switching back to Sampler restores your old trim.",
  };
}

export function projectJsonExplain(): ExplainerText {
  return {
    title: "Project JSON — save and restore the whole setup",
    body: "A JSON dump of the whole setup: sampler configs, mixer levels/mutes, sample comments, page metadata and theme.\n\nCopy to save; Load or paste + Apply to restore. Samples are referenced by path, never embedded — files must exist under ASSETS/SourceSamples/.",
  };
}

export function downloadWavExplain(): ExplainerText {
  return {
    title: "Save .WAV — export the mix",
    body: "In SAMPLER MODE it renders an offline mixdown (trim/ADSR/polyphony/mixer honoured) and downloads it as .wav. In CHIP MODE it downloads the original Furnace-rendered .wav, unchanged.",
  };
}

export function pianoRollExplain(): ExplainerText {
  return {
    title: "Piano Roll — what's sounding right now",
    body: "A keyboard sized to this song's range. A held key lights up in its instrument's colour, the same as the pattern grid swatches.\n\nCH3 (Noise) isn't shown as a key — its \"note\" is a noise rate, not a pitch — it's shown as text instead.",
  };
}

export function coverArtExplain(): ExplainerText {
  return {
    title: "Cover Art — animated, driven by the same triggers as everything else",
    body: "A 240×240 canvas, dithered from a 32×32 buffer with a 4×4 Bayer matrix. A CD spins over flashing Matrix-style code.\n\nEvery note trigger fires a scan-flash around the disc's rim, coloured and sized by its instrument and volume — all four channels, including Noise. Click to save a crisp 1600×1600 PNG.",
  };
}

export function patternsExplain(song: SongModel): ExplainerText {
  return {
    title: "Patterns — the actual note data",
    body: `All four channels at one order position, like Furnace's own view. Each has its own order list (${song.meta.orderLength} positions) of ${song.meta.patternLength}-row patterns.\n\nRow shading follows this song's highlights (${song.meta.highlightA}/${song.meta.highlightB}). OFF = note off; ... / .. / .... = empty note / ins-vol / effect.`,
  };
}

export function rowExplain(song: SongModel, order: number, row: number): ExplainerText {
  const speed = song.meta.speedPattern[0] ?? 6;
  const rowDur = speed / Math.max(song.meta.tickRate, 1);
  const absRow = order * song.meta.patternLength + row;
  return {
    title: `Row ${hex2(row)} · order ${order}`,
    body: `At speed ${speed} ticks/row (${song.meta.tickRate.toFixed(
      2,
    )} Hz), lands ~${(absRow * rowDur).toFixed(2)}s into the song. Click to jump playback here.`,
  };
}

export function channelExplain(song: SongModel, channel: number, muted: boolean): ExplainerText {
  const effects = song.channels[channel]?.effectColumns ?? 1;
  return {
    title: `Channel ${channel} · ${CHANNEL_NAMES[channel]}`,
    body: `${CHANNEL_ROLES[channel] ?? ""}\n\nEffect columns: ${effects}. Click the header to mute or unmute this channel.${
      muted ? " Currently muted." : ""
    }`,
  };
}

export function cellExplain(
  song: SongModel,
  channel: number,
  order: number,
  row: number,
  column: EditColumn,
  cell: PatternCell | undefined,
): ExplainerText {
  const label = `CH${channel} · row ${hex2(row)} · ${columnLabel(column)}`;
  if (!cell) return { title: label, body: "Empty cell." };

  if (column.kind === "note") {
    const note = cell.note;
    if (!note) {
      return { title: label, body: "No note event here — any note from an earlier row keeps sounding." };
    }
    if (note.kind === "off") {
      return {
        title: `${label} · NOTE OFF`,
        body: "Releases the currently sounding note — the envelope decays/cuts from here instead of a new pitch starting.",
      };
    }
    if (note.kind === "release") {
      return { title: `${label} · RELEASE`, body: "Applies the instrument's musical release from this row." };
    }
    if (note.kind === "macroRelease") {
      return {
        title: `${label} · MACRO RELEASE`,
        body: "A chip macro release — a hardware command, not a sampler note-off.",
      };
    }
    if (note.kind === "rawFreq") {
      return {
        title: `${label} · FRQ ${note.value}`,
        body: `Direct frequency override of ${note.value} Hz (format version 248+).`,
      };
    }
    const freq = noteToFreq(note, song.meta.tuningA4) ?? 0;
    const ins = cell.instrument ?? null;
    return {
      title: `${label} · ${noteToName(note)}`,
      body: `note = ${noteToName(note)} (raw ${note.note})\n≈ ${freq.toFixed(2)} Hz (tuning ${song.meta.tuningA4} Hz).\n${
        ins !== null ? `Instrument ${ins} · ${instrumentDescription(song, ins)}.` : "No instrument set yet on this channel."
      }`,
    };
  }

  if (column.kind === "ins") {
    if (cell.instrument === null) {
      return { title: label, body: "No change — keeps the prior instrument." };
    }
    return {
      title: `${label} · instrument ${cell.instrument}`,
      body: `Selects ${cell.instrument} · ${instrumentDescription(
        song,
        cell.instrument,
      )} from here until the next change.`,
    };
  }

  if (column.kind === "vol") {
    if (cell.volume === null) {
      return {
        title: label,
        body: "No override — plays at whatever level the envelope / volume slide already set.",
      };
    }
    return {
      title: `${label} · volume ${cell.volume}`,
      body: `volume = ${cell.volume} / 15 (≈ ${Math.round((cell.volume / 15) * 100)}%) from here until the next volume cell or slide.`,
    };
  }

  const slot = cell.effects[column.index];
  const effect = slot?.effect ?? null;
  const value = slot?.value ?? null;
  if (effect === null && value === null) {
    return { title: label, body: "No effect in this column this row." };
  }
  const entry = FX_CATALOG.find((e) => e.code === effect);
  const meaning = entry
    ? `${entry.description} (${entry.label}).`
    : `Effect 0x${hex2(effect ?? 0)} — parsed and preserved, not documented here yet.`;
  return {
    title: `${label} · 0x${hex2(effect ?? 0)}${hex2(value ?? 0)}`,
    body: `effect = code 0x${hex2(effect ?? 0)}, value 0x${hex2(
      value ?? 0,
    )}.\n${meaning}`,
  };
}
