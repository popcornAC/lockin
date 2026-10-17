<script lang="ts">
  import { onMount } from "svelte";
  import { getBackend, type Backend } from "$lib/backend";
  import { championIcon, championSplash, POSITION_LABELS, positionIcon } from "$lib/assets";
  import type { GameAssets, Settings, Status } from "$lib/types";
  import BuildView from "$lib/components/BuildView.svelte";
  import Img from "$lib/components/Img.svelte";
  import Players from "$lib/components/Players.svelte";
  import SettingsPanel from "$lib/components/SettingsPanel.svelte";

  let backend: Backend | null = null;
  let status = $state<Status | null>(null);
  let settings = $state<Settings | null>(null);
  let assets = $state<GameAssets | null>(null);
  let tiers = $state<[string, string][]>([]);
  let busy = $state(false);
  let tab = $state<"live" | "settings">("live");
  let toast = $state<{ text: string; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let splashFailed = $state<string | null>(null);

  onMount(() => {
    const off: Promise<() => void>[] = [];
    if (new URLSearchParams(location.search).get("tab") === "settings") tab = "settings";
    getBackend().then((b) => {
      backend = b;
      b.getStatus().then((s) => (status = s));
      b.getSettings().then((s) => (settings = s));
      b.tiers().then((t) => (tiers = t));
      b.getAssets().then((a) => (assets = a ?? assets));
      off.push(
        b.on("status", (s) => (status = s)),
        b.on("settings", (s) => (settings = s)),
        b.on("assets", (a) => (assets = a)),
      );
    });
    return () => off.forEach((p) => p.then((f) => f()));
  });

  function notify(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 6000 : 2800);
  }

  async function save() {
    if (!settings || !backend) return;
    try {
      settings = await backend.setSettings($state.snapshot(settings));
    } catch (e) {
      notify(String(e), true);
    }
  }

  async function applyNow() {
    if (!backend) return;
    busy = true;
    try {
      await backend.applyNow();
      notify("Build applied");
    } catch (e) {
      notify(String(e), true);
    } finally {
      busy = false;
    }
  }

  async function refreshBuilds() {
    await backend?.refreshBuilds();
    notify("Build cache cleared for this patch");
  }

  const PHASES: Record<string, { label: string; title: string; hint: string }> = {
    None: { label: "Client open", title: "Client connected", hint: "Head into a Summoner's Rift lobby whenever you're ready." },
    Lobby: { label: "In lobby", title: "In lobby", hint: "Lock in a champion and your build is set up for you." },
    Matchmaking: { label: "In queue", title: "Finding a match", hint: "Builds are prepared when champ select starts." },
    ReadyCheck: { label: "Match found", title: "Match found", hint: "" },
    ChampSelect: { label: "Champ select", title: "Champ select", hint: "" },
    GameStart: { label: "Loading", title: "Game starting", hint: "" },
    InProgress: { label: "In game", title: "In game", hint: "" },
    Reconnect: { label: "Reconnect", title: "Reconnect available", hint: "Your game is still running." },
    WaitingForStats: { label: "Post-game", title: "Game over", hint: "Waiting for stats." },
    PreEndOfGame: { label: "Post-game", title: "Game over", hint: "" },
    EndOfGame: { label: "Post-game", title: "Game over", hint: "Queue up again whenever you're ready." },
  };

  const QUEUES: Record<number, string> = {
    400: "Normal Draft",
    420: "Ranked Solo/Duo",
    430: "Normal Blind",
    440: "Ranked Flex",
    450: "ARAM",
    480: "Swiftplay",
    490: "Quickplay",
    700: "Clash",
    1700: "Arena",
    1900: "URF",
  };
  const MAPS: Record<number, string> = { 12: "ARAM", 30: "Arena" };

  let connected = $derived(status?.connection === "connected");
  let phase = $derived(PHASES[status?.phase ?? "None"] ?? { label: status?.phase ?? "", title: status?.phase ?? "", hint: "" });
  let cs = $derived(status?.champ_select ?? null);
  let imp = $derived(status?.last_import ?? null);
  let inGame = $derived(status?.players_context === "in_game" || status?.phase === "InProgress" || status?.phase === "GameStart");
  let view = $derived(
    !connected ? "offline" : cs ? (cs.supported ? "champ" : "unsupported") : inGame ? "game" : "idle",
  ) as "offline" | "champ" | "unsupported" | "game" | "idle";
  let pill = $derived(!connected ? "Offline" : cs && !cs.supported ? "Unsupported" : phase.label);
  let buildMatches = $derived(!!imp && !!cs && imp.champion_id === cs.champion_id);
  let splash = $derived(cs?.champion_id ? championSplash(assets, cs.champion_id) : null);
  let modeName = $derived(cs && !cs.supported ? (MAPS[cs.map_id ?? 0] ?? QUEUES[cs.queue_id ?? 0] ?? "This mode") : "");
</script>

{#snippet quick()}
  {#if settings}
    <div class="quick" aria-label="Quick toggles">
      {#each [["auto_accept", "Auto-accept"], ["auto_import", "Auto-import"], ["player_lookup", "Player lookup"]] as [key, label]}
        {@const k = key as "auto_accept" | "auto_import" | "player_lookup"}
        <button
          class="chip"
          title="Click to turn {settings[k] ? 'off' : 'on'}"
          role="switch"
          aria-checked={settings[k]}
          onclick={() => {
            if (settings) {
              settings[k] = !settings[k];
              save();
            }
          }}><span class="cdot"></span>{label}</button
        >
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet lastBuild(open: boolean)}
  {#if imp}
    <details class="card collapsible" {open}>
      <summary class="card-head">
        <Img src={championIcon(assets, imp.champion_id)} alt={imp.champion_name} size={20} initials />
        <h2>Last build <span class="h2-sub">{imp.champion_name} {POSITION_LABELS[imp.position] ?? ""}</span></h2>
        <span class="chev" aria-hidden="true"></span>
      </summary>
      <div class="card-body"><BuildView {imp} {assets} /></div>
    </details>
  {/if}
{/snippet}

<div class="app">
  <header class="top">
    <div class="brand">
      <svg class="logo" viewBox="0 0 24 24" aria-hidden="true">
        <defs>
          <linearGradient id="lg" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stop-color="#f0dfae" />
            <stop offset="1" stop-color="#9c7a3c" />
          </linearGradient>
        </defs>
        <path d="M12 1.8 20.8 6.9v10.2L12 22.2 3.2 17.1V6.9z" fill="#0d141f" stroke="url(#lg)" stroke-width="1.4" />
        <path d="M7.6 15.6 15.9 7.3" stroke="url(#lg)" stroke-width="1.6" stroke-linecap="round" />
        <path d="M10.4 17.1 17 10.5" stroke="#3fb7c4" stroke-width="1.6" stroke-linecap="round" />
      </svg>
      <span class="wordmark">Rift <b>Helper</b></span>
    </div>
    <div class="pill {view}" title={connected ? `League client connected${status?.region ? ` · ${status.region}` : ""}` : "Waiting for the League client"}>
      <span class="pdot"></span>{pill}
    </div>
  </header>

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "live"} onclick={() => (tab = "live")}>Live</button>
    <button role="tab" aria-selected={tab === "settings"} onclick={() => (tab = "settings")}>Settings</button>
    {#if status?.patch}<span class="patch">Patch {status.patch}</span>{/if}
  </div>

  <main>
    {#if tab === "settings"}
      {#if settings}
        <SettingsPanel bind:settings {status} {tiers} {save} {refreshBuilds} />
      {/if}
    {:else if !status}
      <div class="empty"><span class="spinner" aria-label="Loading"></span></div>
    {:else if view === "offline"}
      <section class="empty waiting">
        <div class="radar" aria-hidden="true">
          <span></span><span></span><span></span>
          <svg viewBox="0 0 24 24"
            ><path d="M12 1.8 20.8 6.9v10.2L12 22.2 3.2 17.1V6.9z" fill="none" stroke="currentColor" stroke-width="1.2" /><circle
              cx="12"
              cy="12"
              r="2.2"
              fill="currentColor"
            /></svg
          >
        </div>
        <h1>Waiting for League</h1>
        <p>Lockin is idle in your tray and connects as soon as the League client starts.</p>
        {@render quick()}
      </section>
    {:else if view === "unsupported" && cs}
      <section class="card unsupported-card">
        <div class="warn-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24"
            ><path d="M12 3 22 20H2z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" /><path
              d="M12 10v4.5M12 17.2v.3"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
            /></svg
          >
        </div>
        <div>
          <div class="eyebrow">Champ select · {modeName}</div>
          <h1>Not supported in this mode</h1>
          <p>Lockin only works on Summoner's Rift. Nothing will be imported or changed this game.</p>
        </div>
      </section>
    {:else if view === "champ" && cs}
      <section class="hero" class:locked={cs.locked}>
        {#if splash && splashFailed !== splash}
          <img class="splash" src={splash} alt="" onerror={() => (splashFailed = splash)} />
        {/if}
        <div class="hero-body">
          <span class="portrait">
            {#if cs.champion_id}
              <Img src={championIcon(assets, cs.champion_id)} alt={cs.champion_name || "Champion"} size={58} initials />
            {:else}
              <Img src={null} alt="?" label="?" size={58} />
            {/if}
          </span>
          <div class="hero-text">
            <div class="eyebrow">{QUEUES[cs.queue_id ?? 0] ?? "Champ select"}</div>
            <h1>{cs.champion_name || "Pick a champion"}</h1>
            <div class="tags">
              {#if cs.position}
                <span class="tag role"
                  ><Img src={positionIcon(cs.position)} alt="" size={13} shape="bare" label=" " />{POSITION_LABELS[cs.position] ?? cs.position}</span
                >
              {/if}
              {#if cs.champion_id}
                <span class="tag {cs.locked ? 'gold' : 'teal'}">{cs.locked ? "Locked in" : "Hovering"}</span>
              {/if}
              {#if cs.queue_id === 440}<span class="tag">Flex data</span>{/if}
            </div>
          </div>
        </div>
        <button class="primary" onclick={applyNow} disabled={busy || !cs.champion_id || status.importing}>
          {#if status.importing}<span class="spinner small"></span>Importing…{:else}Apply build{/if}
        </button>
      </section>

      <section class="card">
        <div class="card-head"><h2>Build</h2></div>
        <div class="card-body">
          {#if status.importing}
            <div class="pending">
              <span class="spinner"></span>
              <span>Importing runes, spells and items…</span>
            </div>
          {:else if buildMatches && imp}
            <BuildView {imp} {assets} />
          {:else}
            <div class="pending">
              <span class="hint-icon" aria-hidden="true">◆</span>
              <span>
                {#if !cs.champion_id}
                  Hover or lock a champion to get a build.
                {:else if settings?.auto_import}
                  Your build is applied when you lock in.
                {:else}
                  Auto-import is off. Press <b>Apply build</b> to set it up.
                {/if}
              </span>
            </div>
          {/if}
        </div>
      </section>

      {#if status.players.length}
        <section class="card">
          <div class="card-head"><h2>Visible teammates</h2></div>
          <div class="card-body"><Players players={status.players} inGame={false} {assets} /></div>
        </section>
      {/if}
    {:else if view === "game"}
      <section class="card">
        <div class="card-head">
          <h2>Players in this game</h2>
          {#if status.region}<span class="head-note">{status.region} · solo queue rank</span>{/if}
        </div>
        <div class="card-body">
          {#if status.players.length}
            <Players players={status.players} inGame={true} {assets} />
          {:else if settings?.player_lookup}
            <div class="pending"><span class="spinner"></span><span>Waiting for the loading screen…</span></div>
          {:else}
            <div class="pending"><span class="hint-icon" aria-hidden="true">◆</span><span>Player lookup is off.</span></div>
          {/if}
        </div>
      </section>
      {@render lastBuild(false)}
    {:else}
      <section class="card status-card">
        <div class="phase-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24"
            ><path d="M12 1.8 20.8 6.9v10.2L12 22.2 3.2 17.1V6.9z" fill="none" stroke="currentColor" stroke-width="1.3" /><path
              d="m8.3 12.2 2.5 2.5 5-5.2"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
            /></svg
          >
        </div>
        <div>
          <div class="eyebrow">Connected{status.region ? ` · ${status.region}` : ""}</div>
          <h1>{phase.title}</h1>
          {#if phase.hint}<p>{phase.hint}</p>{/if}
        </div>
      </section>
      {@render quick()}
      {#if status.players.length}
        <section class="card">
          <div class="card-head"><h2>Players</h2></div>
          <div class="card-body"><Players players={status.players} inGame={false} {assets} /></div>
        </section>
      {/if}
      {@render lastBuild(true)}
    {/if}
  </main>

  {#if toast}
    <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
  {/if}
</div>

<style>
  .app {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
    background:
      radial-gradient(120% 60% at 50% -10%, rgba(63, 183, 196, 0.07), transparent 60%),
      radial-gradient(90% 50% at 100% 0%, rgba(201, 164, 92, 0.05), transparent 60%),
      var(--bg);
  }

  /* header */
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px 10px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .logo {
    width: 22px;
    height: 22px;
    filter: drop-shadow(0 0 6px rgba(214, 176, 96, 0.25));
  }
  .wordmark {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .wordmark b {
    color: var(--gold-bright);
    font-weight: 700;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px 3px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-2);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    white-space: nowrap;
  }
  .pdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c, var(--ok));
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c, var(--ok)) 22%, transparent);
  }
  .pill.offline {
    --c: var(--faint);
    color: var(--muted);
  }
  .pill.champ {
    --c: var(--gold);
    color: var(--gold-bright);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .pill.champ .pdot,
  .pill.game .pdot {
    animation: pulse 1.8s ease-in-out infinite;
  }
  .pill.game {
    --c: var(--teal);
    color: var(--teal-bright);
    box-shadow: inset 0 0 0 1px var(--teal-line);
  }
  .pill.unsupported {
    --c: var(--warn);
    color: var(--warn);
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 5px color-mix(in srgb, var(--c) 8%, transparent);
    }
  }

  /* tabs */
  .tabs {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 0 14px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    position: relative;
    padding: 6px 10px 8px;
    border: none;
    background: none;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    cursor: pointer;
    transition: color 0.15s;
  }
  .tabs button:hover {
    color: var(--text-2);
  }
  .tabs button::after {
    content: "";
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: -1px;
    height: 2px;
    background: linear-gradient(90deg, transparent, var(--gold), transparent);
    opacity: 0;
    transform: scaleX(0.4);
    transition:
      opacity 0.2s,
      transform 0.2s;
  }
  .tabs button[aria-selected="true"] {
    color: var(--gold-bright);
  }
  .tabs button[aria-selected="true"]::after {
    opacity: 1;
    transform: none;
  }
  .patch {
    margin-left: auto;
    padding-bottom: 9px;
    font-size: 11px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    scrollbar-gutter: stable;
  }
  main > :global(*) {
    flex-shrink: 0;
  }

  /* shared text */
  h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
    letter-spacing: 0.01em;
    color: var(--text);
  }
  p {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 12.5px;
  }
  .eyebrow {
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
    margin-bottom: 2px;
  }

  /* cards */
  .card {
    border-radius: 6px;
    background: linear-gradient(180deg, var(--surface-2), var(--surface) 60px);
    box-shadow:
      inset 0 0 0 1px var(--line),
      0 1px 0 rgba(255, 255, 255, 0.02) inset;
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px 0;
  }
  .card-head h2 {
    margin: 0;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
  }
  .h2-sub {
    color: var(--text-2);
    letter-spacing: 0.04em;
    margin-left: 4px;
  }
  .head-note {
    margin-left: auto;
    font-size: 11px;
    color: var(--faint);
  }
  .card-body {
    padding: 10px 12px 12px;
  }
  .collapsible summary {
    list-style: none;
    cursor: pointer;
    padding-bottom: 9px;
    user-select: none;
    border-radius: 6px;
    transition: background-color 0.15s;
  }
  .collapsible summary:hover {
    background: var(--hover);
  }
  .collapsible summary::-webkit-details-marker {
    display: none;
  }
  .collapsible[open] summary {
    padding-bottom: 0;
  }
  .collapsible[open] summary:hover {
    background: none;
  }
  .chev {
    margin-left: auto;
    width: 6px;
    height: 6px;
    border-right: 1.5px solid var(--gold);
    border-bottom: 1.5px solid var(--gold);
    transform: rotate(-45deg);
    transition: transform 0.15s;
  }
  .collapsible[open] .chev {
    transform: rotate(45deg);
  }

  /* champ select hero */
  .hero {
    position: relative;
    overflow: hidden;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 12px;
    border-radius: 6px;
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    isolation: isolate;
  }
  .hero.locked {
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .splash {
    position: absolute;
    inset: 0;
    z-index: -1;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: 70% 22%;
    opacity: 0.55;
    animation: fade 0.4s ease-out;
    mask-image: linear-gradient(90deg, transparent 0%, rgba(0, 0, 0, 0.5) 35%, #000 75%);
  }
  .hero::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(90deg, var(--surface) 25%, rgba(13, 20, 31, 0.55) 70%, rgba(13, 20, 31, 0.25));
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .hero-body {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .portrait {
    display: grid;
    padding: 2px;
    border-radius: 4px;
    background: linear-gradient(160deg, var(--gold-bright), var(--gold-deep));
    box-shadow: 0 4px 14px -4px rgba(0, 0, 0, 0.8);
  }
  .hero:not(.locked) .portrait {
    background: linear-gradient(160deg, var(--teal-bright), #1d5560);
  }
  .hero-text {
    min-width: 0;
  }
  .hero-text h1 {
    font-size: 19px;
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.7);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tags {
    display: flex;
    gap: 5px;
    margin-top: 5px;
    flex-wrap: wrap;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
    background: rgba(7, 11, 18, 0.65);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    backdrop-filter: blur(4px);
  }
  .tag.gold {
    color: var(--gold-bright);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .tag.teal {
    color: var(--teal-bright);
    box-shadow: inset 0 0 0 1px var(--teal-line);
  }
  .tag.role :global(img) {
    opacity: 0.85;
  }

  /* buttons */
  .primary {
    flex: none;
    align-self: flex-end;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 13px;
    border: none;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #1a1309;
    background: linear-gradient(180deg, #f2dfa8, #c9a45c 55%, #a8853f);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.45),
      0 0 0 1px #6d5426,
      0 4px 14px -6px var(--gold-glow);
    cursor: pointer;
    transition:
      filter 0.15s,
      transform 0.1s,
      box-shadow 0.15s;
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.45),
      0 0 0 1px #6d5426,
      0 4px 18px -4px var(--gold-glow);
  }
  .primary:active:not(:disabled) {
    transform: translateY(1px);
  }
  .primary:disabled {
    cursor: default;
    filter: saturate(0.3) brightness(0.7);
  }

  /* quick toggles */
  .quick {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: none;
    border-radius: 999px;
    font-size: 11.5px;
    color: var(--muted);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    cursor: pointer;
    transition:
      color 0.15s,
      box-shadow 0.15s,
      background-color 0.15s;
  }
  .chip:hover {
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--gold-dim);
  }
  .cdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--faint);
    transition: background-color 0.15s;
  }
  .chip[aria-checked="true"] {
    color: var(--text-2);
    background: var(--gold-wash);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .chip[aria-checked="true"] .cdot {
    background: var(--gold);
    box-shadow: 0 0 6px var(--gold-glow);
  }

  /* empty / waiting */
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 6px;
    padding: 24px 18px;
  }
  .waiting p {
    max-width: 270px;
    margin-bottom: 14px;
  }
  .radar {
    position: relative;
    width: 96px;
    height: 96px;
    display: grid;
    place-items: center;
    margin-bottom: 14px;
    color: var(--gold);
  }
  .radar svg {
    width: 40px;
    height: 40px;
    filter: drop-shadow(0 0 8px rgba(214, 176, 96, 0.35));
  }
  .radar span {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 1px solid var(--teal-line);
    animation: ripple 3.6s cubic-bezier(0.2, 0.6, 0.3, 1) infinite;
    opacity: 0;
  }
  .radar span:nth-child(2) {
    animation-delay: 1.2s;
  }
  .radar span:nth-child(3) {
    animation-delay: 2.4s;
  }
  @keyframes ripple {
    0% {
      transform: scale(0.35);
      opacity: 0.9;
    }
    100% {
      transform: scale(1.15);
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .radar span {
      animation: none;
      opacity: 0.5;
      transform: scale(0.9);
    }
    .radar span:nth-child(2) {
      transform: scale(0.65);
    }
    .radar span:nth-child(3) {
      display: none;
    }
  }

  /* status card / unsupported */
  .status-card,
  .unsupported-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px;
  }
  .phase-icon,
  .warn-icon {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    color: var(--ok);
  }
  .phase-icon svg,
  .warn-icon svg {
    width: 40px;
    height: 40px;
  }
  .unsupported-card {
    align-items: flex-start;
    background: linear-gradient(180deg, rgba(227, 169, 72, 0.08), var(--surface) 80%);
    box-shadow: inset 0 0 0 1px rgba(227, 169, 72, 0.3);
  }
  .unsupported-card .eyebrow {
    color: var(--warn);
  }
  .warn-icon {
    color: var(--warn);
    width: 36px;
    height: 36px;
  }
  .warn-icon svg {
    width: 30px;
    height: 30px;
  }
  .unsupported-card h1 {
    font-size: 15px;
  }

  .pending {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
    color: var(--muted);
    font-size: 12.5px;
  }
  .pending b {
    color: var(--gold-bright);
    font-weight: 600;
  }
  .hint-icon {
    color: var(--gold-dim);
    font-size: 10px;
  }
  .spinner {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--gold);
    animation: spin 0.8s linear infinite;
    flex: none;
  }
  .spinner.small {
    width: 11px;
    height: 11px;
    border-width: 1.5px;
    border-color: rgba(26, 19, 9, 0.3);
    border-top-color: #1a1309;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .toast {
    position: absolute;
    left: 50%;
    bottom: 14px;
    transform: translateX(-50%);
    max-width: calc(100% - 28px);
    padding: 7px 14px;
    border-radius: 4px;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-3);
    box-shadow:
      inset 0 0 0 1px var(--gold-line),
      0 8px 24px -6px rgba(0, 0, 0, 0.8);
    animation: rise 0.2s ease-out;
  }
  .toast.error {
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--bad) 55%, transparent),
      0 8px 24px -6px rgba(0, 0, 0, 0.8);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translate(-50%, 6px);
    }
  }
</style>
