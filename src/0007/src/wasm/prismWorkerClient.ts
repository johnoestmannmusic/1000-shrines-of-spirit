import type { AudioClip } from "@/core/dsp";
import type { SpectralSettings } from "@/core/spectral";
import type { WorkerRequest, WorkerResponse } from "./prismWorkerProtocol";

export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer?: Transferable[]): void;
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  onerror: ((event: ErrorEvent | Event) => void) | null;
  terminate(): void;
}

function realWorker(): WorkerLike {
  return new Worker(new URL("./prism.worker.ts", import.meta.url), {
    type: "module",
  }) as unknown as WorkerLike;
}

interface PendingEntry {
  resolve: (value: AudioClip | undefined) => void;
  reject: (error: Error) => void;
}

/**
 * Request/response client for the prism_dsp Spectral render Worker. Accepts an
 * injected transport so the protocol can be unit-tested without a real Worker
 * (unavailable under Vitest's Node environment).
 */
export class PrismWorkerClient {
  private nextId = 1;
  private pending = new Map<number, PendingEntry>();
  private worker: WorkerLike;

  constructor(worker?: WorkerLike) {
    this.worker = worker ?? realWorker();
    this.worker.onmessage = (event) => {
      const data = event.data;
      const entry = this.pending.get(data.id);
      if (!entry) return;
      this.pending.delete(data.id);
      if (data.kind === "error") entry.reject(new Error(data.error));
      else if (data.kind === "result") entry.resolve(data.result);
      else entry.resolve(undefined);
    };
    this.worker.onerror = (event) => {
      const message =
        "message" in event && typeof event.message === "string" && event.message
          ? event.message
          : "prism_dsp worker error";
      for (const entry of this.pending.values()) entry.reject(new Error(message));
      this.pending.clear();
    };
  }

  /** Confirms the worker started and the WASM module initialised, without rendering anything. */
  ping(timeoutMs = 5000): Promise<void> {
    const id = this.nextId++;
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("prism_dsp worker did not respond in time"));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: () => {
          clearTimeout(timer);
          resolve();
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.worker.postMessage({ id, kind: "ping" });
    });
  }

  render(a: AudioClip, b: AudioClip | null, settings: SpectralSettings): Promise<AudioClip> {
    const id = this.nextId++;
    // Copy channel data before transferring: transfer detaches the underlying
    // buffer, but the caller's clips (cached decoded source samples) are
    // reused elsewhere and must not be zeroed out by this call.
    const aCopy: AudioClip = { channels: a.channels.map((c) => c.slice()), sampleRate: a.sampleRate };
    const bCopy: AudioClip | null = b
      ? { channels: b.channels.map((c) => c.slice()), sampleRate: b.sampleRate }
      : null;
    const transfer: Transferable[] = aCopy.channels.map((c) => c.buffer);
    if (bCopy) transfer.push(...bCopy.channels.map((c) => c.buffer));
    return new Promise<AudioClip>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: AudioClip | undefined) => void, reject });
      this.worker.postMessage({ id, kind: "render", a: aCopy, b: bCopy, settings }, transfer);
    });
  }

  terminate(): void {
    this.worker.terminate();
  }
}
