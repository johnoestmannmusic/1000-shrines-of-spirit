import type { LanternApi } from "../shared/types";

declare global {
  interface Window {
    lantern: LanternApi;
  }
}

export {};
