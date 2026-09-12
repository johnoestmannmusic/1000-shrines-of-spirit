import init, { render_fused } from "@/renderer/vendor/prism/prism_wasm.js";
import { makeSpectralRenderer, type PrismWasmModule, type SyncSpectralRenderFn } from "./prism";
import type { WorkerRequest, WorkerResponse } from "./prismWorkerProtocol";

interface WorkerScope {
  postMessage(message: WorkerResponse, transfer?: Transferable[]): void;
  onmessage: ((event: MessageEvent<WorkerRequest>) => void) | null;
}

const scope = self as unknown as WorkerScope;

let ready: Promise<SyncSpectralRenderFn> | null = null;

function ensureReady(): Promise<SyncSpectralRenderFn> {
  if (!ready) {
    ready = init().then(() => makeSpectralRenderer({ render_fused } as unknown as PrismWasmModule));
  }
  return ready;
}

scope.onmessage = (event) => {
  const request = event.data;
  ensureReady()
    .then((render) => {
      if (request.kind === "ping") {
        scope.postMessage({ id: request.id, kind: "pong" });
        return;
      }
      const result = render(request.a, request.b, request.settings);
      const transfer = result.channels.map((channel) => channel.buffer);
      scope.postMessage({ id: request.id, kind: "result", result }, transfer);
    })
    .catch((error: unknown) => {
      scope.postMessage({ id: request.id, kind: "error", error: String(error) });
    });
};
