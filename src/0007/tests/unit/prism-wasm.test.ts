import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { initSync, render_fused } from "@/renderer/vendor/prism/prism_wasm.js";
import { makeSpectralRenderer, type PrismWasmModule } from "@/wasm/prism";
import { defaultSpectralSettings } from "@/core/spectral";
import { projectPath } from "./fixtures";

beforeAll(() => {
  const bytes = readFileSync(projectPath("src/renderer/vendor/prism/prism_wasm_bg.wasm"));
  initSync({ module: bytes });
});

describe("prism_dsp WASM (real engine)", () => {
  it("freezes a sine into a finite stereo loop", () => {
    const rate = 44_100;
    const data = Float32Array.from({ length: rate }, (_, i) =>
      Math.sin((2 * Math.PI * 220 * i) / rate) * 0.5,
    );
    const settings = defaultSpectralSettings();
    settings.mode = "off";
    settings.loopLengthSeconds = 1;

    const wasm = { render_fused } as unknown as PrismWasmModule;
    const clip = makeSpectralRenderer(wasm)({ channels: [data], sampleRate: rate }, null, settings);

    expect(clip.channels.length).toBe(2);
    const duration = clip.channels[0]!.length / clip.sampleRate;
    expect(Math.abs(duration - 1)).toBeLessThan(0.05);
    expect(Array.from(clip.channels[0]!).every(Number.isFinite)).toBe(true);
    expect(Array.from(clip.channels.flatMap((c) => Array.from(c))).some((v) => Math.abs(v) > 1e-6)).toBe(true);
  });

  it("cross-synthesises two samples without non-finite output", () => {
    const rate = 44_100;
    const tone = (freq: number) =>
      Float32Array.from({ length: rate / 2 }, (_, i) =>
        Math.sin((2 * Math.PI * freq * i) / rate) * 0.5,
      );
    const settings = defaultSpectralSettings();
    settings.mode = "cross-synth";
    settings.sourceIndex2 = 1;
    settings.loopLengthSeconds = 0.5;

    const wasm = { render_fused } as unknown as PrismWasmModule;
    const clip = makeSpectralRenderer(wasm)(
      { channels: [tone(220)], sampleRate: rate },
      { channels: [tone(330)], sampleRate: rate },
      settings,
    );
    expect(Array.from(clip.channels.flatMap((c) => Array.from(c))).every(Number.isFinite)).toBe(true);
  });
});
