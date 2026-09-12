import init, { render_fused } from "./prism_wasm.js";
import { registerPrismWasm, registerPrismWasmWorker, type PrismWasmModule } from "@/wasm/prism";
import { PrismWorkerClient } from "@/wasm/prismWorkerClient";

let initialized = false;

/**
 * Lazily initialises the prism_dsp Spectral engine and registers the renderer.
 * Prefers the off-thread Worker so a Spectral render never blocks the UI; if
 * the Worker can't be started or its WASM module fails to initialise, falls
 * back to running prism_dsp on the main thread exactly as before.
 */
export async function initPrismWasm(): Promise<boolean> {
  if (initialized) return true;
  try {
    const client = new PrismWorkerClient();
    await client.ping();
    registerPrismWasmWorker(client);
    initialized = true;
    return true;
  } catch (error) {
    console.warn("prism_dsp Worker failed to initialise, falling back to the main thread:", error);
  }
  try {
    await init();
    registerPrismWasm({ render_fused } as unknown as PrismWasmModule);
    initialized = true;
    return true;
  } catch (error) {
    console.warn("prism_dsp WASM failed to initialise:", error);
    return false;
  }
}
