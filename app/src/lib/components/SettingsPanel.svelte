<!-- The Settings tab: toggles, rank filter, refresh builds, connection info, log. -->
<script lang="ts">
  import type { Settings, Status } from "$lib/types";
  import Toggle from "./Toggle.svelte";

  let {
    settings = $bindable(),
    status,
    tiers,
    save,
    refreshBuilds,
  }: {
    settings: Settings;
    status: Status | null;
    tiers: [string, string][];
    save: () => void;
    refreshBuilds: () => Promise<void>;
  } = $props();

  let refreshing = $state(false);
  let tierLabel = $derived(tiers.find(([v]) => v === settings.opgg_tier)?.[1] ?? "");

  async function refresh() {
    refreshing = true;
    try {
      await refreshBuilds();
    } finally {
      setTimeout(() => (refreshing = false), 600);
    }
  }
</script>

<div class="settings">
  <section class="group">
    <h2>Champ select</h2>
    <div class="box">
      <Toggle bind:checked={settings.auto_accept} onchange={save} label="Auto-accept ready check" hint="Accepts the match pop for you" />
      <Toggle bind:checked={settings.auto_import} onchange={save} label="Apply build when I lock in" hint="Otherwise use Apply build in champ select" />
      <div class="sub">
        <Toggle compact bind:checked={settings.import_runes} onchange={save} label="Rune page" />
        <Toggle compact bind:checked={settings.import_spells} onchange={save} label="Summoner spells" />
        <Toggle compact bind:checked={settings.import_items} onchange={save} label="Item set" />
      </div>
    </div>
  </section>

  <section class="group">
    <h2>Builds</h2>
    <div class="box">
      <Toggle bind:checked={settings.use_opgg} onchange={save} label="Use OP.GG builds" hint="Falls back to the client's own recommendations" />
      {#if settings.use_opgg}
        <div class="field">
          <label for="tier">Rank filter</label>
          <span class="select">
            <select id="tier" bind:value={settings.opgg_tier} onchange={save}>
              {#each tiers as [value, label]}<option {value}>{label}</option>{/each}
            </select>
          </span>
        </div>
      {/if}
      <div class="field source">
        <span class="src">
          {#if settings.use_opgg}
            <b>OP.GG</b> {tierLabel}{status?.patch ? ` · patch ${status.patch}` : ""}<br /><span class="then">then client recommendations</span>
          {:else}
            <b>Client recommendations</b>
          {/if}
        </span>
        {#if settings.use_opgg}
          <button class="btn" onclick={refresh} disabled={refreshing}>
            <svg viewBox="0 0 16 16" aria-hidden="true" class:spin={refreshing}
              ><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2.5v3h-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg
            >
            Refresh builds
          </button>
        {/if}
      </div>
    </div>
  </section>

  <section class="group">
    <h2>Players</h2>
    <div class="box">
      <Toggle bind:checked={settings.player_lookup} onchange={save} label="Look up players" hint="Rank, win rate and top champions from OP.GG" />
      {#if settings.player_lookup}
        <div class="sub">
          <Toggle compact bind:checked={settings.detect_premades} onchange={save} label="Detect premades" hint="Your party, plus likely parties in game from recent OP.GG games" />
        </div>
      {/if}
    </div>
  </section>

  <section class="group">
    <h2>App</h2>
    <div class="box">
      <Toggle bind:checked={settings.launch_at_login} onchange={save} label="Start at login" hint="Starts quietly in the tray" />
      {#if status?.connection === "connected"}
        <div class="field info">
          <span>Client connection</span>
          <span class="val">
            {status.tls === "pinned_riot_root" ? "Certificate verified" : "Localhost only, unverified"}{status.region ? ` · ${status.region}` : ""}
          </span>
        </div>
      {/if}
    </div>
  </section>

  <details class="group log">
    <summary><h2>Activity log</h2><span class="chev" aria-hidden="true"></span></summary>
    <pre>{(status?.log ?? []).join("\n") || "Nothing yet."}</pre>
  </details>

  <footer>
    Lockin isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in
    producing or managing Riot Games properties. Riot Games and all associated properties are trademarks or registered trademarks of
    Riot Games, Inc. Build and player data from OP.GG. Game images from Data Dragon and CommunityDragon.
  </footer>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0 0 6px 2px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
  }
  .box {
    display: flex;
    flex-direction: column;
    padding: 3px;
    border-radius: 6px;
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .sub {
    margin: 0 0 4px 12px;
    padding-left: 6px;
    border-left: 1px solid var(--line-strong);
  }
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
  }
  .field label {
    color: var(--text);
  }
  .select {
    position: relative;
  }
  .select::after {
    content: "";
    position: absolute;
    right: 10px;
    top: 50%;
    width: 6px;
    height: 6px;
    border-right: 1.5px solid var(--gold);
    border-bottom: 1.5px solid var(--gold);
    transform: translateY(-70%) rotate(45deg);
    pointer-events: none;
  }
  select {
    appearance: none;
    min-width: 130px;
    padding: 5px 28px 5px 10px;
    border: none;
    border-radius: 4px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text);
    cursor: pointer;
    transition: box-shadow 0.15s;
  }
  select:hover {
    box-shadow: inset 0 0 0 1px var(--gold-dim);
  }
  option {
    background: var(--surface-2);
  }
  .source {
    border-top: 1px solid var(--line);
    margin: 2px 9px 0;
    padding: 9px 3px 7px;
  }
  .src {
    font-size: 11.5px;
    color: var(--muted);
    line-height: 1.45;
  }
  .src b {
    color: var(--teal-bright);
    font-weight: 600;
  }
  .then {
    color: var(--faint);
  }
  .btn {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border: none;
    border-radius: 4px;
    font-size: 12px;
    color: var(--gold-bright);
    background: var(--gold-wash);
    box-shadow: inset 0 0 0 1px var(--gold-line);
    cursor: pointer;
    transition:
      background-color 0.15s,
      box-shadow 0.15s;
  }
  .btn:hover:not(:disabled) {
    background: rgba(201, 164, 92, 0.2);
    box-shadow: inset 0 0 0 1px var(--gold);
  }
  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .btn svg {
    width: 13px;
    height: 13px;
  }
  .spin {
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .info {
    font-size: 12px;
    color: var(--muted);
  }
  .info .val {
    color: var(--text-2);
    text-align: right;
  }
  .log summary {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    list-style: none;
    user-select: none;
  }
  .log summary::-webkit-details-marker {
    display: none;
  }
  .log summary h2 {
    margin-bottom: 0;
  }
  .chev {
    width: 5px;
    height: 5px;
    border-right: 1.5px solid var(--gold);
    border-bottom: 1.5px solid var(--gold);
    transform: rotate(-45deg);
    transition: transform 0.15s;
  }
  .log[open] .chev {
    transform: rotate(45deg);
  }
  pre {
    margin: 8px 0 0;
    padding: 8px 10px;
    max-height: 200px;
    overflow: auto;
    font: 11px/1.5 ui-monospace, "SF Mono", "Cascadia Mono", Consolas, monospace;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
    background: var(--inset);
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  footer {
    font-size: 10px;
    line-height: 1.5;
    color: var(--faint);
    padding: 0 2px 4px;
  }
</style>
