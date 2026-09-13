import { describe, expect, it } from "vitest";
import { parseFurFile } from "@/core/fur/node";
import {
  applyEdit,
  applySnapshot,
  buildSongModel,
  cellAt,
  patternSnapshot,
  retime,
  type SongModel,
} from "@/core/songModel";
import { rowDurationSec, songPositionAt, rowTime } from "@/core/timing";
import { DEFAULT_ENTRY_NOTE } from "@/core/pitch";
import {
  LOOKAHEAD_SEC,
  setSpectralEnabled,
  Scheduler,
  defaultSamplerSettings,
  envelopeAt,
  loopChannel,
  samplePosition,
  sequenceDuration,
  sequenceFromSong,
  waveform,
} from "@/core/sampler";
import { fixtureBytes, fixtureText } from "./fixtures";
import {
  applyTimingOverrides,
  projectFromJson,
  projectToJson,
  validateProject,
} from "@/core/project";

function fixture(): SongModel {
  return buildSongModel(parseFurFile(fixtureBytes("tests/fixtures/flight_school_night_shift.fur")));
}

describe("song model", () => {
  it("builds from the bundled fixture", () => {
    const song = fixture();
    expect(song.meta.name).toBe("flight_school_night_shift");
    expect(song.channels).toHaveLength(4);
    expect(song.instruments).toHaveLength(10);

    const colors = new Set(song.instruments.map((i) => i.colorRgb.join(",")));
    expect(colors.size).toBe(song.instruments.length);

    const ch0 = song.channels[0]!;
    expect(ch0.insTimeline).toHaveLength(ch0.orderList.length);
    expect(ch0.insTimeline[0]).toHaveLength(song.meta.patternLength);
    expect(ch0.insTimeline[0]![0]).toBe(0);
    expect(ch0.noteTimeline[0]![0]).not.toBeNull();

    const rowDur = rowDurationSec(song.meta);
    expect(rowDur).toBeGreaterThan(0);
    const pos = songPositionAt(song, 0);
    expect(pos).toEqual({ orderPos: 0, row: 0 });
    const pos2 = songPositionAt(song, rowDur * song.meta.patternLength);
    expect(pos2.orderPos).toBe(1);
    expect(pos2.row).toBe(0);
  });

  it("keeps the channel instrument across a note-off", () => {
    const song = fixture();
    const empty = () => ({
      note: null,
      instrument: null,
      volume: null,
      effects: Array.from({ length: 8 }, () => ({ effect: null, value: null })),
    });
    applyEdit(song, {
      channel: 0,
      order: 0,
      row: 0,
      cell: { ...empty(), note: { kind: "note", note: 129 }, instrument: 0 },
    });
    applyEdit(song, {
      channel: 0,
      order: 0,
      row: 1,
      cell: { ...empty(), note: { kind: "off" } },
    });
    applyEdit(song, {
      channel: 0,
      order: 0,
      row: 2,
      cell: { ...empty(), note: { kind: "note", note: 127 } },
    });
    // A note after a note-off must still resolve to the held instrument.
    expect(song.channels[0]!.insTimeline[0]![2]).toBe(0);
    // The held note is still cleared by the OFF.
    expect(song.channels[0]!.noteTimeline[0]![1]).toBeNull();
  });

  it("applyEdit mutates the cell and regenerates timelines", () => {
    const song = fixture();
    applyEdit(song, {
      channel: 0,
      order: 0,
      row: 5,
      cell: {
        note: { kind: "note", note: 108 },
        instrument: 0,
        volume: 15,
        effects: Array.from({ length: 8 }, () => ({ effect: null, value: null })),
      },
    });
    const after = cellAt(song, 0, 0, 5);
    expect(after.note).toEqual({ kind: "note", note: 108 });
    expect(after.instrument).toBe(0);
    expect(after.volume).toBe(15);
    expect(song.channels[0]!.noteTimeline[0]![5]).toEqual({ kind: "note", note: 108 });
    expect(song.channels[0]!.insTimeline[0]![5]).toBe(0);
  });

  it("retime recomputes row times from a changed tick rate", () => {
    const song = fixture();
    const originalSecondRow = song.rowTimes[1]!;
    song.meta.tickRate *= 2;
    retime(song);
    expect(Math.abs(song.rowTimes[1]! - originalSecondRow / 2)).toBeLessThan(1e-9);
  });

  it("pattern snapshot round-trips through applySnapshot", () => {
    const song = fixture();
    const before = patternSnapshot(song);
    const song2 = fixture();
    applySnapshot(song2, before);
    expect(patternSnapshot(song2)).toEqual(before);
    expect(song2.rowTimes).toEqual(song.rowTimes);
    for (let i = 0; i < song.channels.length; i++) {
      expect(song2.channels[i]!.noteTimeline).toEqual(song.channels[i]!.noteTimeline);
      expect(song2.channels[i]!.insTimeline).toEqual(song.channels[i]!.insTimeline);
    }
  });

  it("golden-battletrain applies the F0 tempo lane", () => {
    const song = buildSongModel(
      parseFurFile(fixtureBytes("tests/fixtures/golden-battletrain/06-golden_battletrain.fur")),
    );
    const flat =
      song.meta.orderLength * song.meta.patternLength * rowDurationSec(song.meta);
    const duration = song.rowTimes[song.rowTimes.length - 1]!;
    expect(duration).toBeLessThan(flat);
    expect(Math.abs(duration - 94.416349)).toBeLessThan(0.01);
    for (let i = 1; i < song.rowTimes.length; i++) {
      expect(song.rowTimes[i]!).toBeGreaterThan(song.rowTimes[i - 1]!);
    }
  });
});

describe("sampler", () => {
  const sequence = () => ({
    tuning: 440,
    rows: [[], [], [], []],
    rowTimes: [0, 0.1, 0.2, 0.3, 0.4],
  });

  it("preroll never schedules before the transport anchor", () => {
    const s = new Scheduler(sequence(), 0.05, 0);
    const rows = s.tick(0, LOOKAHEAD_SEC);
    expect(rows[0]!.row).toBe(0);
    expect(rows[0]!.when).toBe(0.05);
    expect(rows.every((r) => r.when >= 0.05)).toBe(true);
    expect(s.tick(0, LOOKAHEAD_SEC)).toEqual([]);
  });

  it("loops keep scheduling without duplicates", () => {
    const s = new Scheduler(sequence(), 2, 0);
    const rows: { row: number; when: number }[] = [];
    for (let i = 0; i <= 80; i++) rows.push(...s.tick(2 + i * 0.025, LOOKAHEAD_SEC));
    expect(rows.length).toBeGreaterThan(20);
    rows.forEach((r, i) => {
      expect(r.row).toBe(i % 4);
      expect(Math.abs(r.when - (2 + i * 0.1))).toBeLessThan(1e-9);
    });
  });

  it("seek mid-row and late wake are bounded", () => {
    const s = new Scheduler(sequence(), 10.05, 0.25);
    const rows = s.tick(10, LOOKAHEAD_SEC);
    expect(rows[0]!.row).toBe(2);
    expect(rows[0]!.when).toBe(10.05);
    const rows2 = s.tick(110, LOOKAHEAD_SEC);
    expect(rows2.length).toBeLessThanOrEqual(3);
    expect(rows2.every((r) => r.when >= 110)).toBe(true);
  });

  it("fixture sequence uses actual note cells and held instruments", () => {
    const song = fixture();
    const seq = sequenceFromSong(song);
    expect(seq.rows).toHaveLength(song.meta.orderLength * song.meta.patternLength);
    const first = seq.rows[0]![0]!;
    expect(first.type).toBe("note");
    if (first.type === "note") {
      expect(first.channel).toBe(0);
      expect(first.instrument).toBe(0);
    }
  });

  it("envelope has a click-safe attack and reaches sustain", () => {
    const s = defaultSamplerSettings();
    s.attack = 0.1;
    s.decay = 0.2;
    s.sustain = 0.25;
    expect(envelopeAt(s, -0.1)).toBe(0);
    expect(Math.abs(envelopeAt(s, 0.05) - 0.5)).toBeLessThan(1e-6);
    expect(Math.abs(envelopeAt(s, 0.2) - 0.625)).toBeLessThan(1e-6);
    expect(envelopeAt(s, 1)).toBe(0.25);
  });

  it("ping-pong loop mirrors and declicks both seams", () => {
    const data = new Float32Array(20).fill(1);
    const out = loopChannel(data, 1000, 0, 0.02, true);
    expect(out.length).toBe(40);
    expect(out[0]).toBe(0);
    expect(out[19]).toBe(0);
    expect(out[20]).toBe(0);
    expect(out[39]).toBe(0);
    for (let i = 0; i < 20; i++) expect(out[i]).toBe(out[39 - i]);
    expect(out[10]).toBe(1);
    expect(loopChannel(data, 1000, 1, 2, false).length).toBe(0);
  });

  it("ping-pong playhead reverses and rate changes duration", () => {
    const s = defaultSamplerSettings();
    s.startSec = 1;
    s.endSec = 2;
    s.looping = true;
    s.pingPong = true;
    expect(samplePosition(s, 0.25, 2)).toBe(1.5);
    expect(samplePosition(s, 0.75, 2)).toBe(1.5);
    expect(samplePosition(s, 1, 2)).toBe(1);
  });

  it("waveform bins min/max", () => {
    const data = Float32Array.from([-1, 1, -0.5, 0.5]);
    const wf = waveform(data, 2);
    expect(wf).toEqual([
      [-1, 1],
      [-0.5, 0.5],
    ]);
  });
});

describe("project json", () => {
  it("original project round-trips without losing its schema", () => {
    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    validateProject(project, 10);
    expect(project.sourceSamples).toHaveLength(6);
    expect(project.instruments).toHaveLength(10);
    expect(project.songTitle).toBe("0006 - alpha _CD");
    expect(project.instruments[0]!.spectral.mode).toBe("off");

    const encoded = projectToJson(project);
    expect(encoded).toContain('"samplerModeEnabled"');
    expect(encoded).toContain('"sourceIndex2"');
    expect(encoded).not.toContain('"muted"');
    const reparsed = projectFromJson(encoded);
    expect(reparsed).toEqual(project);
  });

  it("legacy rootNote converts to transpose", () => {
    const value = JSON.parse(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson")) as {
      instruments: Array<Record<string, unknown>>;
    };
    const first = value.instruments[0]!;
    delete first.transpose;
    first.rootNote = 117;
    const project = projectFromJson(JSON.stringify(value));
    expect(project.instruments[0]!.transpose).toBe(12);
  });

  it("switching Spectral on loops by default and restores the sampler loop on exit", () => {
    const settings = defaultSamplerSettings();
    settings.looping = false;
    setSpectralEnabled(settings, true);
    expect(settings.spectral.enabled).toBe(true);
    expect(settings.looping).toBe(true);
    setSpectralEnabled(settings, false);
    expect(settings.spectral.enabled).toBe(false);
    expect(settings.looping).toBe(false);
  });

  it("round-trips master FX settings", () => {
    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    project.masterFx.delay.enabled = true;
    project.masterFx.delay.timeSec = 0.19;
    project.masterFx.delay.feedback = 0.5;
    project.masterFx.reverb.enabled = true;
    project.masterFx.reverb.decaySec = 3.5;
    const reread = projectFromJson(projectToJson(project));
    expect(reread.masterFx).toEqual(project.masterFx);
  });

  it("round-trips vibrato settings", () => {
    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    project.instruments[0]!.vibratoSpeed = 7.5;
    project.instruments[0]!.vibratoDepth = 0.4;
    const reread = projectFromJson(projectToJson(project));
    expect(reread.instruments[0]!.vibratoSpeed).toBe(7.5);
    expect(reread.instruments[0]!.vibratoDepth).toBe(0.4);
  });

  it("round-trips instrument display names", () => {
    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    project.instrumentNames = ["Bass 1", "Lead", "Perc"];
    const reread = projectFromJson(projectToJson(project));
    expect(reread.instrumentNames).toEqual(["Bass 1", "Lead", "Perc"]);
  });

  it("round-trips the instrument pan centre and random width", () => {
    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    project.instruments[0]!.pan = -0.5;
    project.instruments[0]!.panRandomRange = 0.25;
    const reread = projectFromJson(projectToJson(project));
    expect(reread.instruments[0]!.pan).toBe(-0.5);
    expect(reread.instruments[0]!.panRandomRange).toBe(0.25);
  });

  it("gracefully imports the legacy Fusion schema and unknown modes", () => {
    const value = JSON.parse(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson")) as {
      instruments: Array<Record<string, unknown>>;
    };
    const first = value.instruments[0]!;
    delete first.spectral;
    first.spectralFusion = {
      enabled: true,
      mode: "spectral-blend",
      algorithm: "spectral-blend",
      freezePoint: 30,
    };
    delete first.transpose;
    first.rootNote = 117;
    value.instruments[1]!.spectral = { mode: "totally-unknown" };

    const project = projectFromJson(JSON.stringify(value));
    expect(project.instruments[0]!.transpose).toBe(12); // rootNote migration
    expect(project.instruments[0]!.spectral.enabled).toBe(true);
    expect(project.instruments[0]!.spectral.mode).toBe("cross-synth");
    expect(project.instruments[1]!.spectral.mode).toBe("off");
  });

  it("applyTimingOverrides only touches fields that are set", () => {
    const song = fixture();
    const originalSecondRow = song.rowTimes[1]!;
    const originalTickRate = song.meta.tickRate;

    applyTimingOverrides(projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson")), song);
    expect(song.meta.tickRate).toBe(originalTickRate);

    const project = projectFromJson(fixtureText("tests/fixtures/lmp-default-proj.legacy.lampjson"));
    project.tickRateOverride = originalTickRate * 2;
    project.speedOverride = 3;
    project.highlightAOverride = 8;
    project.highlightBOverride = 16;
    project.virtualTempoOverride = [2, 1];
    applyTimingOverrides(project, song);
    expect(song.meta.tickRate).toBe(originalTickRate * 2);
    expect(song.meta.speedPattern[0]).toBe(3);
    expect(song.meta.highlightA).toBe(8);
    expect(song.meta.highlightB).toBe(16);
    expect(song.meta.virtualTempo).toEqual([2, 1]);
    expect(song.rowTimes[1]).not.toBe(originalSecondRow);
  });
});
