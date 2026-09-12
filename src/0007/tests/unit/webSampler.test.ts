import { afterEach, describe, expect, it } from "vitest";
import { SamplerEngine } from "@/audio/webSampler";
import {
  defaultSpectralSettings,
  makeClip,
  registerSpectralRenderer,
  setSpectralWasmAvailable,
  type SpectralRenderFn,
} from "@/core/spectral";
import { defaultSamplerSettings } from "@/core/sampler";
import type { AudioClip } from "@/core/dsp";

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function fakeAudioContext(): AudioContext {
  return {
    createBuffer(channelCount: number, length: number) {
      const channels = Array.from({ length: channelCount }, () => new Float32Array(length));
      return { getChannelData: (c: number) => channels[c]! };
    },
  } as unknown as AudioContext;
}

afterEach(() => {
  registerSpectralRenderer(null);
  setSpectralWasmAvailable(false);
});

describe("SamplerEngine.renderSpectral", () => {
  it("discards a stale render superseded by a newer one for the same instrument", async () => {
    const pending: Array<{ resolve: (clip: AudioClip) => void }> = [];
    const renderer: SpectralRenderFn = () => {
      const d = deferred<AudioClip>();
      pending.push(d);
      return d.promise;
    };
    registerSpectralRenderer(renderer);
    setSpectralWasmAvailable(true);

    const engine = new SamplerEngine();
    const settings = defaultSamplerSettings();
    settings.sourceIndex = 0;
    settings.spectral = { ...defaultSpectralSettings(), enabled: true, mode: "off" };
    engine.settings = [settings];
    engine.clips = [makeClip([[1, 2, 3]], 44_100)];

    const ctx = fakeAudioContext();
    const first = engine.renderSpectral(ctx, 0);
    expect(engine.rendering[0]).toBe(true);
    const second = engine.renderSpectral(ctx, 0);
    expect(engine.rendering[0]).toBe(true);
    expect(pending).toHaveLength(2);

    const clipA = makeClip([[0.1]], 44_100);
    const clipB = makeClip([[0.9]], 44_100);

    // Resolve the first (now-stale) render before the second: its result must be discarded.
    pending[0]!.resolve(clipA);
    await first;
    expect(engine.fusedClips[0]).toBeNull();
    expect(engine.rendering[0]).toBe(true); // the second render is still in flight

    pending[1]!.resolve(clipB);
    await second;
    expect(engine.fusedClips[0]).toBe(clipB);
    expect(engine.rendering[0]).toBe(false);
    expect(engine.takeFusionCompleted(0)).toBe(true);
    expect(engine.takeFusionCompleted(0)).toBe(false); // one-shot: cleared after being read
  });
});
