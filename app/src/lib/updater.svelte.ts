// In-app updates via the official Tauri updater plugin. A quiet check runs at
// start and every few hours; installing (download, install, relaunch) only
// happens when the user clicks. In a plain browser (`vite dev`, mock mode) a fake
// updater is used: add `?update=0.9.0` to preview "update available", or
// `?update=error` to preview a failed check.

/** Phases where swapping the app out would get in the way. */
const BUSY_PHASES = new Set(["ReadyCheck", "ChampSelect", "GameStart", "InProgress", "Reconnect"]);
const RECHECK_MS = 6 * 60 * 60 * 1000;

export function installBlocked(phase: string | undefined): string | null {
  return phase && BUSY_PHASES.has(phase) ? "You can install after champ select or your game." : null;
}

interface Found {
  version: string;
  /** Downloads and installs, reporting progress (0..1, or null if unknown), then relaunches. */
  install(progress: (p: number | null) => void): Promise<void>;
}

interface Source {
  currentVersion(): Promise<string>;
  check(): Promise<Found | null>;
}

async function tauriSource(): Promise<Source> {
  const { getVersion } = await import("@tauri-apps/api/app");
  const { check } = await import("@tauri-apps/plugin-updater");
  const { relaunch } = await import("@tauri-apps/plugin-process");
  return {
    currentVersion: getVersion,
    check: async () => {
      const u = await check();
      if (!u) return null;
      return {
        version: u.version,
        install: async (progress) => {
          // A dev build isn't an installed app bundle; don't let it overwrite anything.
          if (import.meta.env.DEV) throw new Error("Installing is off in dev builds");
          let total = 0;
          let done = 0;
          await u.downloadAndInstall((e) => {
            if (e.event === "Started") total = e.data.contentLength ?? 0;
            else if (e.event === "Progress") done += e.data.chunkLength;
            progress(total ? Math.min(done / total, 1) : null);
          });
          // On Windows the installer has already closed the app by now.
          await relaunch();
        },
      };
    },
  };
}

function mockSource(): Source {
  const want = new URLSearchParams(location.search).get("update");
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  return {
    currentVersion: async () => "0.2.0",
    check: async () => {
      await wait(500);
      if (want === "error") throw new Error("Could not fetch a valid release JSON from the remote");
      if (!want) return null;
      return {
        version: want,
        install: async (progress) => {
          for (let i = 1; i <= 10; i++) {
            await wait(150);
            progress(i / 10);
          }
          throw new Error("Mock mode: nothing to install");
        },
      };
    },
  };
}

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current"; checkedAt: Date }
  | { kind: "available"; version: string }
  | { kind: "installing"; version: string; progress: number | null }
  | { kind: "error"; message: string };

export class Updater {
  version = $state<string | null>(null);
  state = $state<UpdateState>({ kind: "idle" });
  #source: Promise<Source>;
  #found: Found | null = null;
  #timer: ReturnType<typeof setInterval> | undefined;

  constructor() {
    const inTauri = "__TAURI_INTERNALS__" in window;
    this.#source = inTauri ? tauriSource() : Promise.resolve(mockSource());
    this.#source.then((s) => s.currentVersion()).then((v) => (this.version = v), () => {});
  }

  /** Quiet check now and every few hours. Returns a stop function. */
  start(): () => void {
    this.check(true);
    this.#timer = setInterval(() => this.check(true), RECHECK_MS);
    return () => clearInterval(this.#timer);
  }

  /** `quiet` checks never show errors; they just leave the last result in place. */
  async check(quiet = false) {
    if (this.state.kind === "checking" || this.state.kind === "installing") return;
    const prev = this.state;
    if (!quiet) this.state = { kind: "checking" };
    try {
      const found = await (await this.#source).check();
      this.#found = found;
      this.state = found ? { kind: "available", version: found.version } : { kind: "current", checkedAt: new Date() };
    } catch (e) {
      this.state = quiet ? prev : { kind: "error", message: String(e).replace(/^Error: /, "") };
    }
  }

  async install() {
    const found = this.#found;
    if (!found || this.state.kind !== "available") return;
    this.state = { kind: "installing", version: found.version, progress: 0 };
    try {
      await found.install((progress) => (this.state = { kind: "installing", version: found.version, progress }));
    } catch (e) {
      this.state = { kind: "error", message: String(e).replace(/^Error: /, "") };
    }
  }
}
