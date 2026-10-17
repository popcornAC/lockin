// The window's view of the Rust side: Tauri commands and events. In `vite dev`
// outside Tauri (a plain browser), a mock backend is used instead so every UI
// state can be previewed; it is never part of a production build.

import type { GameAssets, Settings, Status } from "./types";

type Unlisten = () => void;

export interface Backend {
  getStatus(): Promise<Status>;
  getSettings(): Promise<Settings>;
  setSettings(s: Settings): Promise<Settings>;
  tiers(): Promise<[string, string][]>;
  getAssets(): Promise<GameAssets | null>;
  applyNow(): Promise<void>;
  refreshBuilds(): Promise<void>;
  on(event: "status", cb: (s: Status) => void): Promise<Unlisten>;
  on(event: "settings", cb: (s: Settings) => void): Promise<Unlisten>;
  on(event: "assets", cb: (a: GameAssets) => void): Promise<Unlisten>;
}

async function tauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    getStatus: () => invoke("get_status"),
    getSettings: () => invoke("get_settings"),
    setSettings: (settings) => invoke("set_settings", { settings }),
    tiers: () => invoke("tiers"),
    getAssets: () => invoke("get_assets"),
    applyNow: () => invoke("apply_now"),
    refreshBuilds: () => invoke("refresh_builds"),
    on: (event: string, cb: (p: never) => void) => listen(event, (e) => cb(e.payload as never)),
  };
}

export async function getBackend(): Promise<Backend> {
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { mockBackend } = await import("./mock");
    return mockBackend();
  }
  return tauriBackend();
}
