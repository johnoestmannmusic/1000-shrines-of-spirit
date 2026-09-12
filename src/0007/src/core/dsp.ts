/** Shared decoded-audio container, mirroring lantern-core's `dsp::AudioClip`. */
export interface AudioClip {
  channels: Float32Array[];
  sampleRate: number;
}

export function audioClip(channels: Float32Array[], sampleRate: number): AudioClip {
  let len = channels.length > 0 ? Number.POSITIVE_INFINITY : 0;
  for (const channel of channels) len = Math.min(len, channel.length);
  if (!Number.isFinite(len)) len = 0;
  const trimmed = channels.map((c) => (c.length === len ? c : c.subarray(0, len)));
  return { channels: trimmed, sampleRate };
}

export function clipLen(clip: AudioClip): number {
  return clip.channels[0]?.length ?? 0;
}

export function clipIsEmpty(clip: AudioClip): boolean {
  return clipLen(clip) === 0 || clip.channels.length === 0 || clip.sampleRate === 0;
}

export function clipDuration(clip: AudioClip): number {
  return clipLen(clip) / Math.max(clip.sampleRate, 1);
}
