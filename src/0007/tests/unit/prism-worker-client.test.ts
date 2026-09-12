import { describe, expect, it } from "vitest";
import { PrismWorkerClient, type WorkerLike } from "@/wasm/prismWorkerClient";
import type { WorkerRequest, WorkerResponse } from "@/wasm/prismWorkerProtocol";
import { makeClip } from "@/core/spectral";

class FakeWorker implements WorkerLike {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null = null;
  onerror: ((event: ErrorEvent | Event) => void) | null = null;
  sent: WorkerRequest[] = [];

  constructor(private respond: (request: WorkerRequest) => WorkerResponse | null) {}

  postMessage(message: WorkerRequest): void {
    this.sent.push(message);
    const response = this.respond(message);
    if (response) {
      queueMicrotask(() => this.onmessage?.({ data: response } as MessageEvent<WorkerResponse>));
    }
  }

  terminate(): void {}
}

describe("PrismWorkerClient", () => {
  it("resolves ping once the worker responds pong", async () => {
    const fake = new FakeWorker((req) => ({ id: req.id, kind: "pong" }));
    const client = new PrismWorkerClient(fake);
    await expect(client.ping()).resolves.toBeUndefined();
  });

  it("rejects ping if the worker never responds", async () => {
    const fake = new FakeWorker(() => null);
    const client = new PrismWorkerClient(fake);
    await expect(client.ping(20)).rejects.toThrow(/did not respond/);
  });

  it("resolves render() with the worker's result", async () => {
    const clip = makeClip([[1, 2], [3, 4]], 44_100);
    const fake = new FakeWorker((req) => ({ id: req.id, kind: "result", result: clip }));
    const client = new PrismWorkerClient(fake);
    const result = await client.render(makeClip([[0]], 44_100), null, {} as never);
    expect(result).toBe(clip);
  });

  it("rejects render() with the worker's error", async () => {
    const fake = new FakeWorker((req) => ({ id: req.id, kind: "error", error: "boom" }));
    const client = new PrismWorkerClient(fake);
    await expect(client.render(makeClip([[0]], 44_100), null, {} as never)).rejects.toThrow("boom");
  });

  it("matches concurrent requests to their own response by id", async () => {
    const fake = new FakeWorker(() => null);
    const client = new PrismWorkerClient(fake);

    const clipA = makeClip([[1]], 44_100);
    const clipB = makeClip([[2]], 44_100);
    const first = client.render(makeClip([[0]], 44_100), null, {} as never);
    const second = client.render(makeClip([[0]], 44_100), null, {} as never);

    const [idA, idB] = fake.sent.map((r) => r.id);
    // Respond out of order to confirm each promise resolves with its own result.
    fake.onmessage?.({ data: { id: idB!, kind: "result", result: clipB } } as MessageEvent<WorkerResponse>);
    fake.onmessage?.({ data: { id: idA!, kind: "result", result: clipA } } as MessageEvent<WorkerResponse>);

    expect(await first).toBe(clipA);
    expect(await second).toBe(clipB);
  });

  it("copies clip channel data instead of transferring the caller's buffers", async () => {
    const source = makeClip([[1, 2, 3]], 44_100);
    const fake = new FakeWorker((req) => ({ id: req.id, kind: "pong" }));
    const client = new PrismWorkerClient(fake);
    void client.render(source, null, {} as never);
    const sentRequest = fake.sent[0] as Extract<WorkerRequest, { kind: "render" }>;
    expect(sentRequest.a.channels[0]).not.toBe(source.channels[0]);
    expect(Array.from(sentRequest.a.channels[0]!)).toEqual([1, 2, 3]);
  });

  it("rejects pending requests when the worker errors", async () => {
    const fake = new FakeWorker(() => null);
    const client = new PrismWorkerClient(fake);
    const pending = client.ping(5000);
    fake.onerror?.({ message: "worker crashed" } as ErrorEvent);
    await expect(pending).rejects.toThrow("worker crashed");
  });
});
