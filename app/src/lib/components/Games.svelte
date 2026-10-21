<!-- The Games tab: recent games (from OP.GG, newest first) and, on click, the
     post-game view. Games still waiting for OP.GG show as "Fetching…" rows. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { championIcon, POSITION_LABELS, positionIcon } from "$lib/assets";
  import { clientQueueLabel, duration, kdaRatio, perMin, queueLabel, timeAgo } from "$lib/games";
  import type { GameAssets, GameRecord, GamesView } from "$lib/types";
  import Img from "./Img.svelte";
  import PostGame from "./PostGame.svelte";

  let {
    view,
    assets,
    loadGame,
    refresh,
    initialId = null,
  }: {
    view: GamesView | null;
    assets: GameAssets | null;
    loadGame: (id: string) => Promise<GameRecord | null>;
    refresh: (force: boolean) => void;
    initialId?: string | null;
  } = $props();

  let open = $state<GameRecord | null>(null);
  let opening = $state<string | null>(null);
  let failed = $state<string | null>(null);
  let now = $state(Date.now() / 1000);

  onMount(() => {
    if (initialId) show(initialId);
    const t = setInterval(() => (now = Date.now() / 1000), 30_000);
    return () => clearInterval(t);
  });

  async function show(id: string) {
    opening = id;
    failed = null;
    try {
      const g = await loadGame(id);
      if (g) open = g;
      else failed = id;
    } catch {
      failed = id;
    } finally {
      opening = null;
    }
  }

  let games = $derived(view?.games ?? []);
  let pending = $derived(view?.pending ?? []);
</script>

{#if open}
  <PostGame game={open} {assets} {now} back={() => (open = null)} />
{:else}
  <section class="card">
    <div class="card-head">
      <h2>Recent games</h2>
      {#if view?.account}<span class="head-note" title="Games for {view.account}, from OP.GG">{view.account}</span>{/if}
      <button
        class="icon-btn"
        class:spin={view?.syncing}
        title={view?.syncing ? "Checking OP.GG…" : "Check OP.GG for new games"}
        aria-label="Refresh games"
        disabled={view?.syncing || !view?.account}
        onclick={() => refresh(true)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /><path
            d="M12.4 1.6v2.9H9.5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          /></svg
        >
      </button>
    </div>
    <div class="card-body list-body">
      {#if !view}
        <div class="pending-note"><span class="spinner"></span>Loading…</div>
      {:else if !games.length && !pending.length}
        <div class="empty-list">
          {#if view.syncing}
            <span class="spinner"></span>
            <p>Loading your recent games from OP.GG…</p>
          {:else if !view.account}
            <p>Your Summoner's Rift games show up here after the League client connects.</p>
          {:else}
            <p>No Summoner's Rift games yet. Finished games appear here a few minutes after they end.</p>
          {/if}
        </div>
      {:else}
        <ul class="games">
          {#each pending as p (p.lcu_game_id)}
            <li class="game fetching">
              <span class="res-bar"></span>
              {#if p.champion_id}
                <Img src={championIcon(assets, p.champion_id)} alt="Champion" size={36} initials />
              {:else}
                <Img src={null} alt="" label="?" size={36} />
              {/if}
              <div class="mid">
                <div class="line1"><span class="spinner tiny"></span><span class="res">Fetching…</span></div>
                <div class="line2">{clientQueueLabel(p.queue_id)} · waiting for OP.GG</div>
              </div>
              <div class="right"><span class="ago">{timeAgo(p.ended_at, now)}</span></div>
            </li>
          {/each}
          {#each games as g (g.id)}
            {@const res = g.win === null ? "" : g.win ? "win" : "loss"}
            <li>
              <button class="game {res}" onclick={() => show(g.id)} disabled={opening === g.id} aria-label="Open {g.champion} game">
                <span class="res-bar"></span>
                <span class="champ">
                  <Img src={championIcon(assets, g.champion_id)} alt={g.champion} size={36} initials />
                  {#if positionIcon(g.position)}
                    <span class="pos" title={POSITION_LABELS[g.position]}
                      ><Img src={positionIcon(g.position)} alt={POSITION_LABELS[g.position] ?? ""} size={12} shape="bare" label=" " /></span
                    >
                  {/if}
                </span>
                <div class="mid">
                  <div class="line1">
                    <span class="res">{g.win === null ? "Game" : g.win ? "Victory" : "Defeat"}</span>
                    <span class="queue">{queueLabel(g.queue)}</span>
                  </div>
                  <div class="line2">
                    <span class="kda"><b>{g.kills}</b> / <b class="d">{g.deaths}</b> / <b>{g.assists}</b></span>
                    <span class="ratio">{kdaRatio(g.kills, g.deaths, g.assists)}{g.deaths ? " KDA" : ""}</span>
                  </div>
                </div>
                <div class="right">
                  <span class="cs">{g.cs} CS <span class="per">{perMin(g.cs, g.duration).toFixed(1)}</span></span>
                  <span class="ago">{duration(g.duration)} · {timeAgo(g.ended_at, now)}</span>
                </div>
                {#if opening === g.id}<span class="spinner tiny opening"></span>{/if}
              </button>
              {#if failed === g.id}<div class="row-err">Couldn't open this game.</div>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if view?.error && !view.syncing}
        <p class="sync-err" title={view.error}>Couldn't reach OP.GG. Showing saved games.</p>
      {/if}
    </div>
  </section>
  <p class="foot">Stats from OP.GG, usually a few minutes after a game ends. Summoner's Rift only.</p>
{/if}

<style>
  .card {
    border-radius: 6px;
    background: linear-gradient(180deg, var(--surface-2), var(--surface) 60px);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px 0;
    min-width: 0;
  }
  .card-head h2 {
    margin: 0;
    flex: none;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
  }
  .head-note {
    margin-left: auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--faint);
  }
  .icon-btn {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-left: auto;
    padding: 0;
    border: none;
    border-radius: 4px;
    color: var(--muted);
    background: none;
    cursor: pointer;
    transition:
      color 0.15s,
      background-color 0.15s;
  }
  .head-note + .icon-btn {
    margin-left: 0;
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--gold-bright);
    background: var(--hover);
  }
  .icon-btn:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .icon-btn svg {
    width: 14px;
    height: 14px;
  }
  .icon-btn.spin svg {
    animation: spin 0.9s linear infinite;
  }
  .card-body {
    padding: 8px 6px 8px;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .games {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .game {
    position: relative;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 6px 10px;
    border: none;
    border-radius: 4px;
    background: none;
    text-align: left;
    color: inherit;
    font: inherit;
    --res: var(--faint);
  }
  button.game {
    cursor: pointer;
    transition: background-color 0.15s;
  }
  button.game:hover {
    background: var(--hover);
  }
  .game.win {
    --res: var(--teal);
    background: linear-gradient(90deg, var(--teal-wash), transparent 45%);
  }
  .game.loss {
    --res: var(--red);
    background: linear-gradient(90deg, rgba(224, 96, 90, 0.08), transparent 45%);
  }
  .game.win:hover,
  .game.loss:hover {
    background-color: var(--hover);
  }
  .res-bar {
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 3px;
    border-radius: 2px;
    background: var(--res);
  }
  .champ {
    position: relative;
    display: grid;
    flex: none;
  }
  .pos {
    position: absolute;
    right: -4px;
    bottom: -4px;
    display: grid;
    place-items: center;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: var(--bg);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .mid {
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: baseline;
    gap: 7px;
    white-space: nowrap;
  }
  .res {
    font-weight: 650;
    font-size: 13px;
    color: var(--res);
  }
  .fetching .res {
    color: var(--text-2);
  }
  .fetching .line1 {
    align-items: center;
  }
  .queue {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .line2 {
    display: flex;
    align-items: baseline;
    gap: 7px;
    margin-top: 1px;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .kda {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .kda b {
    color: var(--text);
    font-weight: 600;
  }
  .kda b.d {
    color: var(--bad);
  }
  .ratio {
    font-size: 11px;
    color: var(--faint);
  }
  .right {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    font-variant-numeric: tabular-nums;
  }
  .cs {
    font-size: 12px;
    color: var(--text-2);
  }
  .per {
    color: var(--faint);
    font-size: 11px;
  }
  .ago {
    font-size: 11px;
    color: var(--faint);
    white-space: nowrap;
  }
  .opening {
    position: absolute;
    right: 8px;
    top: 8px;
  }
  .row-err,
  .sync-err {
    margin: 2px 10px 4px;
    font-size: 11.5px;
    color: var(--bad);
  }
  .sync-err {
    margin-top: 8px;
    color: var(--warn);
  }
  .empty-list {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 18px 16px 14px;
    text-align: center;
  }
  .empty-list p {
    margin: 0;
    max-width: 270px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .pending-note {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    color: var(--muted);
  }
  .foot {
    margin: 0;
    padding: 0 4px;
    font-size: 11px;
    color: var(--faint);
    text-align: center;
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
  .spinner.tiny {
    width: 11px;
    height: 11px;
    border-width: 1.5px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
