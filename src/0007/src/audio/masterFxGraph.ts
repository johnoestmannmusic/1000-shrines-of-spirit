import type { MasterFxSettings } from "@/core/masterFx";

export interface MasterFxGraph {
  input: AudioNode;
  output: AudioNode;
  update(settings: MasterFxSettings): void;
}

const DEFAULT_IR_DECAY = 2.0;

/**
 * Builds the master-output effect chain (dry + delay + convolution reverb)
 * on any `BaseAudioContext` (live `AudioContext` or `OfflineAudioContext`),
 * so playback and export share exactly the same effects.
 */
export function createMasterFxGraph(
  ctx: BaseAudioContext,
  initial: MasterFxSettings,
): MasterFxGraph {
  const input = ctx.createGain();
  const output = ctx.createGain();
  input.connect(output); // dry

  const delayNode = ctx.createDelay(5.0);
  const delayTone = ctx.createBiquadFilter();
  delayTone.type = "lowpass";
  const delayFeedback = ctx.createGain();
  const delayWet = ctx.createGain();
  input.connect(delayNode);
  delayNode.connect(delayTone);
  delayTone.connect(delayWet);
  delayWet.connect(output);
  delayTone.connect(delayFeedback);
  delayFeedback.connect(delayNode);

  const reverbConvolver = ctx.createConvolver();
  const reverbWet = ctx.createGain();
  input.connect(reverbConvolver);
  reverbConvolver.connect(reverbWet);
  reverbWet.connect(output);

  let current = initial;
  let lastDecay = -1;

  const regenerateImpulse = () => {
    const decay = Math.max(0.1, current.reverb.decaySec || DEFAULT_IR_DECAY);
    if (Math.abs(decay - lastDecay) < 1e-6) return;
    lastDecay = decay;
    const length = Math.max(1, Math.floor(ctx.sampleRate * decay));
    const buffer = ctx.createBuffer(2, length, ctx.sampleRate);
    for (let c = 0; c < 2; c++) {
      const data = buffer.getChannelData(c);
      for (let i = 0; i < length; i++) {
        data[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / length, 2.2);
      }
    }
    reverbConvolver.buffer = buffer;
  };

  const update = (settings: MasterFxSettings) => {
    current = settings;
    const now = ctx.currentTime;
    delayNode.delayTime.setValueAtTime(Math.max(0.001, settings.delay.timeSec), now);
    delayFeedback.gain.setValueAtTime(
      settings.delay.enabled ? Math.min(Math.max(settings.delay.feedback, 0), 0.95) : 0,
      now,
    );
    delayTone.frequency.setValueAtTime(Math.max(200, settings.delay.toneHz), now);
    delayWet.gain.setValueAtTime(
      settings.delay.enabled ? Math.min(Math.max(settings.delay.mix, 0), 1) : 0,
      now,
    );
    reverbWet.gain.setValueAtTime(
      settings.reverb.enabled ? Math.min(Math.max(settings.reverb.mix, 0), 1) : 0,
      now,
    );
    regenerateImpulse();
  };

  update(initial);
  return { input, output, update };
}
