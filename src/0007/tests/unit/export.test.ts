import { describe, expect, it } from "vitest";
import { crc32, wavPcm16, zipStore } from "@/core/export";
import { makeClip } from "@/core/spectral";
import { clipDuration } from "@/core/dsp";
import { buildSongModel } from "@/core/songModel";
import { parseFurFile } from "@/core/fur/node";
import { writeMidi } from "@/core/midi";
import { defaultSamplerSettings } from "@/core/sampler";
import { renderSamplerMix } from "@/core/export";
import { fixtureBytes } from "./fixtures";

function ascii(bytes: Uint8Array, start: number, length: number): string {
  return String.fromCharCode(...bytes.subarray(start, start + length));
}

function view(bytes: Uint8Array): DataView {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

describe("WAV export", () => {
  it("writes a canonical 16-bit PCM header and asymmetric sample scaling", () => {
    const clip = makeClip([[0, 0.5, -0.5, 1, -1]], 8000);
    const bytes = wavPcm16(clip);
    const data = view(bytes);

    expect(ascii(bytes, 0, 4)).toBe("RIFF");
    expect(ascii(bytes, 8, 4)).toBe("WAVE");
    expect(ascii(bytes, 12, 4)).toBe("fmt ");
    expect(ascii(bytes, 36, 4)).toBe("data");

    expect(data.getUint16(20, true)).toBe(1); // PCM
    expect(data.getUint16(22, true)).toBe(1); // channels
    expect(data.getUint32(24, true)).toBe(8000); // sample rate
    expect(data.getUint16(34, true)).toBe(16); // bits per sample
    expect(data.getUint32(40, true)).toBe(10); // data size
    expect(data.getUint32(4, true)).toBe(36 + 10);

    expect(data.getInt16(44, true)).toBe(0);
    expect(data.getInt16(46, true)).toBe(Math.trunc(0.5 * 32767));
    expect(data.getInt16(48, true)).toBe(Math.trunc(-0.5 * 32768));
    expect(data.getInt16(50, true)).toBe(32767);
    expect(data.getInt16(52, true)).toBe(-32768);
  });
});

describe("ZIP export", () => {
  it("computes the standard CRC-32 of 123456789", () => {
    expect(crc32(new TextEncoder().encode("123456789"))).toBe(0xcbf43926);
  });

  it("writes STORE-method local, central and EOCD records", () => {
    const payload = new TextEncoder().encode("hello");
    const bytes = zipStore([{ name: "a.txt", data: payload }]);
    const data = view(bytes);

    expect(data.getUint32(0, true)).toBe(0x04034b50);
    expect(ascii(bytes, 30, 5)).toBe("a.txt");
    expect(ascii(bytes, 35, 5)).toBe("hello");

    let centralIndex = -1;
    for (let i = 0; i <= bytes.length - 4; i++) {
      if (data.getUint32(i, true) === 0x02014b50) {
        centralIndex = i;
        break;
      }
    }
    expect(centralIndex).toBeGreaterThan(0);
    expect(ascii(bytes, centralIndex + 46, 5)).toBe("a.txt");

    expect(ascii(bytes, bytes.length - 22, 4)).toBe("PK\u0005\u0006");
    expect(data.getUint16(bytes.length - 22 + 10, true)).toBe(1); // total entries
  });
});

describe("MIDI export", () => {
  it("writes a format-1 SMF with one tempo track and one track per channel", () => {
    const song = buildSongModel(parseFurFile(fixtureBytes("assets/flight_school_night_shift.fur")));
    const bytes = writeMidi(song);
    const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

    expect(ascii(bytes, 0, 4)).toBe("MThd");
    expect(data.getUint32(4, false)).toBe(6);
    expect(data.getUint16(8, false)).toBe(1); // format 1
    expect(data.getUint16(10, false)).toBe(1 + Math.min(song.channels.length, 4));
    expect(data.getUint16(12, false)).toBe(24); // TICKS_PER_ROW
    expect(ascii(bytes, 14, 4)).toBe("MTrk");
    // Each track ends with an End of Track meta event.
    expect(ascii(bytes, bytes.length - 3, 3)).toBe("\u00ff\u002f\u0000");
  });
});

describe("offline sampler mixdown", () => {
  it("renders a stereo clip from a simple note", () => {
    const settings = defaultSamplerSettings();
    settings.sourceIndex = 0;
    settings.endSec = 1;
    settings.attack = 0.003;
    settings.decay = 0;
    settings.sustain = 1;
    settings.volume = 1;

    const sequence = {
      tuning: 440,
      rows: [[{ type: "note" as const, channel: 0, instrument: 0, rate: 1, volume: 1 }]],
      rowTimes: [0, 1],
    };
    const samples = new Float32Array(44_100).fill(0.5);
    const clip = makeClip([Array.from(samples)], 44_100);

    const mix = renderSamplerMix(
      sequence,
      [settings],
      [clip],
      [1, 1, 1, 1],
      [false, false, false, false],
      1,
    );
    expect(mix.channels).toHaveLength(2);
    expect(clipDuration(mix)).toBeGreaterThan(0.9);
    const peak = Math.max(...Array.from(mix.channels[0]!, (v) => Math.abs(v)));
    expect(peak).toBeGreaterThan(0);
  });
});
