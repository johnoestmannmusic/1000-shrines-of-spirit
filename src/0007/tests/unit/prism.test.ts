import { afterEach, describe, expect, it } from "vitest";
import {
  makeClip,
  registerSpectralRenderer,
  setSpectralWasmAvailable,
  spectralRender,
  defaultSpectralSettings,
} from "@/core/spectral";
import { makeSpectralRenderer, registerPrismWasm, type PrismWasmModule } from "@/wasm/prism";

afterEach(() => {
  registerSpectralRenderer(null);
  setSpectralWasmAvailable(false);
});

describe("prism_dsp WASM adapter", () => {
  it("marshals clip channels and spectral settings into render_fused", () => {
    const calls: unknown[][] = [];
    const wasm: PrismWasmModule = {
      render_fused(...args) {
        calls.push(args);
        return {
          channelCount: 2,
          sampleRate: 44_100,
          data: Float32Array.from([0.5, -0.5, 0.25, -0.25]),
        };
      },
    };
    const settings = defaultSpectralSettings();
    settings.mode = "mix";
    settings.freezePoint = 42;
    settings.stereoWidth = 70;

    const a = makeClip([[1, 2, 3]], 44_100);
    const b = makeClip([[4, 5, 6]], 44_100);
    const clip = makeSpectralRenderer(wasm)(a, b, settings);

    expect(calls).toHaveLength(1);
    const args = calls[0]!;
    expect(Array.from(args[0] as Float32Array)).toEqual([1, 2, 3]);
    expect(args[1]).toBe(1);
    expect(Array.from(args[2] as Float32Array)).toEqual([4, 5, 6]);
    expect(args[3]).toBe(1);
    expect(args[4]).toBe(44_100);
    expect(args[5]).toBe(42);
    expect(args[9]).toBe("mix");
    expect(args[18]).toBe(70);

    expect(clip.channels).toHaveLength(2);
    expect(Array.from(clip.channels[0]!)).toEqual([0.5, -0.5]);
    expect(Array.from(clip.channels[1]!)).toEqual([0.25, -0.25]);
  });

  it("registers a renderer that peak-normalises its output", async () => {
    const wasm: PrismWasmModule = {
      render_fused() {
        return {
          channelCount: 1,
          sampleRate: 44_100,
          data: Float32Array.from([0.25, -0.5, 0.25]),
        };
      },
    };
    registerPrismWasm(wasm);
    const clip = await spectralRender(makeClip([[1, 1]], 44_100), null, defaultSpectralSettings());
    const peak = Math.max(...Array.from(clip.channels[0]!, (v) => Math.abs(v)));
    expect(Math.abs(peak - 1)).toBeLessThan(1e-4);
  });
});
