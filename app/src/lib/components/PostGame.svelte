<!-- Post-game view for one stored game: result header, your stats, lane
     comparison and both teams' scoreboard. Only what OP.GG reports: no LP
     change (not provided), no estimates. -->
<script lang="ts">
  import { championIcon, championSplash, itemIcon, itemName, POSITION_LABELS, positionIcon, rune, spell } from "$lib/assets";
  import {
    compact,
    damageShare,
    duration,
    kdaRatio,
    killParticipation,
    laneOpponent,
    perMin,
    queueLabel,
    splitRiotId,
    teamPlayers,
    timeAgo,
  } from "$lib/games";
  import type { GameAssets, GameRecord, PlayerRecord } from "$lib/types";
  import Img from "./Img.svelte";

  let { game, assets, now, back }: { game: GameRecord; assets: GameAssets | null; now: number; back: () => void } = $props();

  type SortKey = "role" | "damage" | "gold" | "cs" | "wards";
  const SORTS: [SortKey, string][] = [
    ["role", "Role"],
    ["damage", "Damage"],
    ["gold", "Gold"],
    ["cs", "CS"],
    ["wards", "Wards"],
  ];
  let sort = $state<SortKey>("role");

  let me = $derived(game.me !== null ? (game.players[game.me] ?? null) : null);
  let myTeam = $derived(me ? (game.teams.find((t) => t.side === me.side) ?? null) : null);
  let win = $derived(myTeam?.win ?? null);
  let opp = $derived(me ? laneOpponent(game, me) : null);
  let splash = $derived(me ? championSplash(assets, me.champion_id) : null);
  let splashFailed = $state<string | null>(null);
  let mins = $derived(game.duration / 60);

  const statValue = (p: PlayerRecord, k: SortKey): number =>
    k === "damage" ? p.damage : k === "gold" ? p.gold : k === "cs" ? p.cs : k === "wards" ? p.wards_placed : 0;
  // The stat shown under each row's KDA: the sorted one, damage by default.
  let shown = $derived<Exclude<SortKey, "role">>(sort === "role" ? "damage" : sort);
  let maxShown = $derived(Math.max(1, ...game.players.map((p) => statValue(p, shown))));

  // OP.GG convention: MVP = best score in the game, ACE = best on the losing team.
  let mvp = $derived(game.players.find((p) => p.op_rank === 1) ?? null);
  let ace = $derived.by(() => {
    const losers = game.teams.find((t) => !t.win);
    if (!losers) return null;
    const list = teamPlayers(game, losers.side).filter((p) => p.op_rank !== null);
    return list.length ? list.reduce((a, b) => ((a.op_rank ?? 99) <= (b.op_rank ?? 99) ? a : b)) : null;
  });

  let teams = $derived(
    game.teams.map((t) => {
      const list = teamPlayers(game, t.side);
      if (sort !== "role") list.sort((a, b) => statValue(b, sort) - statValue(a, sort));
      return { team: t, list };
    }),
  );

  let cards = $derived.by(() => {
    if (!me) return [];
    const kp = killParticipation(game, me);
    const share = damageShare(game, me);
    return [
      { label: "KDA", value: kdaRatio(me.kills, me.deaths, me.assists), sub: `${me.kills} / ${me.deaths} / ${me.assists}` },
      { label: "Kill part.", value: kp === null ? "–" : `${Math.round(kp * 100)}%`, sub: `${myTeam?.kills ?? 0} team kills` },
      { label: "CS", value: String(me.cs), sub: `${perMin(me.cs, game.duration).toFixed(1)} / min` },
      { label: "Damage", value: compact(me.damage), sub: share === null ? "to champions" : `${Math.round(share * 100)}% of team` },
      { label: "Gold / min", value: String(Math.round(perMin(me.gold, game.duration))), sub: `${compact(me.gold)} total` },
      { label: "Wards", value: String(me.wards_placed), sub: `${me.control_wards} control bought` },
    ];
  });

  let lane = $derived.by(() => {
    if (!me || !opp) return [];
    const rows: { label: string; a: number; b: number; fmt: (n: number) => string; lowerIsBetter?: boolean }[] = [
      { label: "Kills + assists", a: me.kills + me.assists, b: opp.kills + opp.assists, fmt: String },
      { label: "Deaths", a: me.deaths, b: opp.deaths, fmt: String, lowerIsBetter: true },
      { label: "CS", a: me.cs, b: opp.cs, fmt: String },
      { label: "Damage", a: me.damage, b: opp.damage, fmt: compact },
      { label: "Gold", a: me.gold, b: opp.gold, fmt: compact },
      { label: "Wards", a: me.wards_placed, b: opp.wards_placed, fmt: String },
    ];
    return rows;
  });

  function fmtStat(p: PlayerRecord, k: Exclude<SortKey, "role">): string {
    const v = statValue(p, k);
    return k === "cs" ? `${v} CS` : k === "wards" ? `${v} wards` : k === "gold" ? `${compact(v)} g` : compact(v);
  }

  function score(p: PlayerRecord): string {
    return p.op_score === null ? "" : p.op_score >= 10 ? "10" : p.op_score.toFixed(1);
  }
</script>

<div class="pg">
  <button class="back" onclick={back}><span class="chev" aria-hidden="true"></span>Games</button>

  <section class="hero" class:win={win === true} class:loss={win === false}>
    {#if splash && splashFailed !== splash}
      <img class="splash" src={splash} alt="" onerror={() => (splashFailed = splash)} />
    {/if}
    <div class="hero-body">
      {#if me}
        <span class="portrait"><Img src={championIcon(assets, me.champion_id)} alt={me.champion} size={52} initials /></span>
      {/if}
      <div class="hero-text">
        <div class="eyebrow">{queueLabel(game.queue)} · {timeAgo(game.ended_at, now)}</div>
        <h1>{win === null ? "Game" : win ? "Victory" : "Defeat"}</h1>
        <div class="tags">
          {#if me}
            <span class="tag">{me.champion}</span>
            {#if me.position}
              <span class="tag role"
                ><Img src={positionIcon(me.position)} alt="" size={12} shape="bare" label=" " />{POSITION_LABELS[me.position] ?? me.position}</span
              >
            {/if}
          {/if}
          <span class="tag">{duration(game.duration)}</span>
          {#if me?.op_score !== null && me?.op_score !== undefined}
            <span class="tag gold" title="OP.GG performance score (0-10), rank {me.op_rank ?? '–'} of 10">OP {score(me)}{me.op_rank ? ` · #${me.op_rank}` : ""}</span>
          {/if}
        </div>
      </div>
    </div>
  </section>

  {#if me}
    <section class="stats" aria-label="Your stats">
      {#each cards as c}
        <div class="stat">
          <span class="s-label">{c.label}</span>
          <span class="s-value">{c.value}</span>
          <span class="s-sub">{c.sub}</span>
        </div>
      {/each}
    </section>
  {/if}

  {#if me && opp}
    <section class="card">
      <div class="card-head">
        <h2>Vs lane opponent</h2>
        <span class="head-note">{POSITION_LABELS[me.position] ?? ""}</span>
      </div>
      <div class="card-body">
        <div class="vs-head">
          <span class="who mine"><Img src={championIcon(assets, me.champion_id)} alt={me.champion} size={22} shape="round" initials />{me.champion} <span class="dim">(you)</span></span>
          <span class="who theirs">{opp.champion}<Img src={championIcon(assets, opp.champion_id)} alt={opp.champion} size={22} shape="round" initials /></span>
        </div>
        <ul class="vs">
          {#each lane as r}
            {@const total = r.a + r.b}
            {@const share = total > 0 ? r.a / total : 0.5}
            {@const better = r.a === r.b ? 0 : (r.a > r.b) !== !!r.lowerIsBetter ? 1 : -1}
            <li>
              <span class="va" class:lead={better > 0}>{r.fmt(r.a)}</span>
              <div class="vbar">
                <span class="vlabel">{r.label}</span>
                <span class="track"><span class="fill-a" style:width="{share * 100}%"></span><span class="fill-b"></span></span>
              </div>
              <span class="vb" class:lead={better < 0}>{r.fmt(r.b)}</span>
            </li>
          {/each}
        </ul>
      </div>
    </section>
  {/if}

  <section class="card">
    <div class="card-head">
      <h2>Scoreboard</h2>
      {#if game.average_tier}<span class="head-note" title="Average rank in this game (OP.GG)">Avg {game.average_tier.charAt(0) + game.average_tier.slice(1).toLowerCase()}</span>{/if}
    </div>
    <div class="sorts" role="radiogroup" aria-label="Sort scoreboard">
      {#each SORTS as [k, label]}
        <button role="radio" aria-checked={sort === k} class="sort" onclick={() => (sort = k)}>{label}</button>
      {/each}
    </div>
    <div class="card-body board">
      {#each teams as { team, list } (team.side)}
        <div class="team {team.side === 'BLUE' ? 'blue' : 'red'}">
          <div class="team-head">
            <span class="bar"></span>
            <span class="tname">{team.side === "BLUE" ? "Blue" : "Red"} · <span class:won={team.win}>{team.win ? "Victory" : "Defeat"}</span></span>
            <span class="tstats">
              <span title="Kills">{team.kills} K</span>
              <span title="Gold">{compact(team.gold)} g</span>
              <span title="Towers / dragons / barons">{team.towers} T · {team.dragons} D · {team.barons} B</span>
            </span>
          </div>
          <ul>
            {#each list as p (p.riot_id + p.champion_id)}
              {@const [name, tag] = splitRiotId(p.riot_id)}
              {@const isMe = me === p}
              {@const items = [...p.items.slice(0, 6), ...Array(Math.max(0, 6 - p.items.length)).fill(0)]}
              <li class="row" class:me={isMe}>
                <span class="champ">
                  <Img src={championIcon(assets, p.champion_id)} alt={p.champion} size={30} initials />
                  <span class="lvl">{p.level}</span>
                </span>
                <span class="loadout">
                  {#each [0, 1] as i}
                    {@const s = spell(assets, p.spells[i] ?? 0)}
                    <Img src={s?.icon} alt={s?.name ?? "Spell"} size={14} />
                  {/each}
                  {#each [p.keystone, p.sub_style] as id, i}
                    {@const r = id ? rune(assets, id) : null}
                    <Img src={r?.icon} alt={r?.name ?? "Rune"} size={14} shape={i ? "bare" : "round"} class={i ? "tree" : "keystone"} />
                  {/each}
                </span>
                <div class="pmain">
                  <div class="pname">
                    <span class="gn" title={p.riot_id}>{name}</span><span class="tagl">{tag}</span>
                    {#if isMe}<span class="you">You</span>{/if}
                    {#if mvp === p}<span class="badge mvp" title="Best OP Score in the game">MVP</span>{:else if ace === p}<span class="badge ace" title="Best OP Score on the losing team">ACE</span>{/if}
                  </div>
                  <div class="items">
                    {#each items as id}
                      {#if id}
                        <Img src={itemIcon(assets, id)} alt={itemName(assets, id)} size={16} />
                      {:else}
                        <span class="slot"></span>
                      {/if}
                    {/each}
                    {#if p.role_item}
                      <Img src={itemIcon(assets, p.role_item)} alt={itemName(assets, p.role_item)} size={16} shape="round" class="role-item" />
                    {/if}
                  </div>
                </div>
                <div class="pstats">
                  <span class="pkda"><b>{p.kills}</b>/<b class="d">{p.deaths}</b>/<b>{p.assists}</b></span>
                  <span class="pval">{fmtStat(p, shown)}</span>
                  <span class="pbar"><span style:width="{(statValue(p, shown) / maxShown) * 100}%"></span></span>
                </div>
                {#if p.op_score !== null}
                  <span class="op" class:top={p.op_rank !== null && p.op_rank <= 3} title="OP Score {p.op_score} · #{p.op_rank ?? '–'} of 10">{score(p)}</span>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>
  </section>
  <p class="foot">From OP.GG. OP Score is OP.GG's own 0-10 rating. LP change and vision score aren't available from this source.</p>
</div>

<style>
  .pg {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .back {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: -2px 0 -2px -4px;
    padding: 3px 8px 3px 6px;
    border: none;
    border-radius: 4px;
    background: none;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    cursor: pointer;
    transition:
      color 0.15s,
      background-color 0.15s;
  }
  .back:hover {
    color: var(--gold-bright);
    background: var(--hover);
  }
  .chev {
    width: 6px;
    height: 6px;
    border-left: 1.5px solid currentColor;
    border-bottom: 1.5px solid currentColor;
    transform: rotate(45deg);
  }

  /* header */
  .hero {
    position: relative;
    overflow: hidden;
    padding: 14px 12px;
    border-radius: 6px;
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    isolation: isolate;
    --res: var(--text);
  }
  .hero.win {
    --res: var(--teal-bright);
    box-shadow: inset 0 0 0 1px var(--teal-line);
  }
  .hero.loss {
    --res: #f08a83;
    box-shadow: inset 0 0 0 1px rgba(224, 96, 90, 0.32);
  }
  .splash {
    position: absolute;
    inset: 0;
    z-index: -1;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: 70% 22%;
    opacity: 0.5;
    mask-image: linear-gradient(90deg, transparent 0%, rgba(0, 0, 0, 0.5) 35%, #000 75%);
  }
  .hero::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(90deg, var(--surface) 25%, rgba(13, 20, 31, 0.55) 70%, rgba(13, 20, 31, 0.25));
  }
  .hero-body {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .portrait {
    display: grid;
    padding: 2px;
    border-radius: 4px;
    background: linear-gradient(160deg, var(--gold-bright), var(--gold-deep));
    box-shadow: 0 4px 14px -4px rgba(0, 0, 0, 0.8);
  }
  .hero-text {
    min-width: 0;
  }
  .eyebrow {
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
    margin-bottom: 2px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 650;
    color: var(--res);
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.7);
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
    white-space: nowrap;
  }
  .tag.gold {
    color: var(--gold-bright);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }

  /* stat cards */
  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px 9px 7px;
    border-radius: 5px;
    background: linear-gradient(180deg, var(--surface-2), var(--surface));
    box-shadow: inset 0 0 0 1px var(--line);
    min-width: 0;
  }
  .s-label {
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    white-space: nowrap;
  }
  .s-value {
    font-size: 17px;
    font-weight: 650;
    color: var(--text);
    font-variant-numeric: tabular-nums;
    line-height: 1.25;
  }
  .s-sub {
    font-size: 10.5px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* cards */
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
  }
  .card-head h2 {
    margin: 0;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold);
  }
  .head-note {
    margin-left: auto;
    font-size: 11px;
    color: var(--faint);
    white-space: nowrap;
  }
  .card-body {
    padding: 10px 12px 12px;
  }

  /* lane comparison */
  .vs-head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 6px;
    font-size: 12px;
    font-weight: 600;
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .who.mine {
    color: var(--teal-bright);
  }
  .who.theirs {
    color: #f08a83;
  }
  .dim {
    color: var(--faint);
    font-weight: 500;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .vs {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .vs li {
    display: grid;
    grid-template-columns: 44px 1fr 44px;
    align-items: end;
    gap: 8px;
    font-variant-numeric: tabular-nums;
  }
  .va,
  .vb {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.2;
  }
  .vb {
    text-align: right;
  }
  .va.lead,
  .vb.lead {
    color: var(--text);
    font-weight: 650;
  }
  .vbar {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .vlabel {
    font-size: 10px;
    color: var(--faint);
    text-align: center;
    letter-spacing: 0.04em;
  }
  .track {
    display: flex;
    height: 5px;
    border-radius: 3px;
    overflow: hidden;
    background: var(--inset);
    gap: 2px;
  }
  .fill-a {
    background: linear-gradient(90deg, #1d6a74, var(--teal));
    border-radius: 3px 0 0 3px;
  }
  .fill-b {
    flex: 1;
    background: linear-gradient(90deg, var(--red), #8a3632);
    border-radius: 0 3px 3px 0;
  }

  /* scoreboard */
  .sorts {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
    flex-wrap: wrap;
  }
  .sort {
    padding: 2px 9px;
    border: none;
    border-radius: 999px;
    font-size: 11px;
    color: var(--muted);
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    cursor: pointer;
    transition:
      color 0.15s,
      box-shadow 0.15s;
  }
  .sort:hover {
    color: var(--text-2);
  }
  .sort[aria-checked="true"] {
    color: var(--gold-bright);
    background: var(--gold-wash);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .board {
    padding: 8px 8px 10px;
  }
  .team + .team {
    margin-top: 10px;
  }
  .blue {
    --team: var(--blue);
  }
  .red {
    --team: var(--red);
  }
  .team-head {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 4px 4px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    white-space: nowrap;
  }
  .bar {
    width: 3px;
    height: 11px;
    border-radius: 1px;
    background: var(--team);
  }
  .won {
    color: var(--teal-bright);
  }
  .tstats {
    margin-left: auto;
    display: flex;
    gap: 8px;
    font-weight: 500;
    letter-spacing: 0.02em;
    text-transform: none;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px;
    border-radius: 4px;
  }
  .row + .row {
    border-top: 1px solid rgba(27, 38, 54, 0.6);
  }
  .row.me {
    background: var(--gold-wash);
    box-shadow: inset 0 0 0 1px var(--gold-line);
    border-top-color: transparent;
  }
  .row.me + .row {
    border-top-color: transparent;
  }
  .champ {
    position: relative;
    display: grid;
    flex: none;
  }
  .lvl {
    position: absolute;
    left: -3px;
    bottom: -3px;
    min-width: 15px;
    padding: 0 2px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
    line-height: 13px;
    text-align: center;
    color: var(--text-2);
    background: var(--bg);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .loadout {
    flex: none;
    display: grid;
    grid-template-columns: 14px 14px;
    grid-auto-flow: column;
    grid-template-rows: 14px 14px;
    gap: 2px;
  }
  .loadout :global(.tree) {
    padding: 1px;
  }
  .pmain {
    flex: 1;
    min-width: 0;
  }
  .pname {
    display: flex;
    align-items: baseline;
    gap: 3px;
    min-width: 0;
    font-size: 12px;
    white-space: nowrap;
  }
  .gn {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
    font-weight: 550;
  }
  .tagl {
    flex: none;
    max-width: 44px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 10.5px;
    color: var(--faint);
  }
  .you,
  .badge {
    flex: none;
    margin-left: 3px;
    padding: 0 4px;
    border-radius: 2px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    line-height: 13px;
    text-transform: uppercase;
    align-self: center;
  }
  .you {
    color: #1a1309;
    background: var(--gold);
  }
  .badge.mvp {
    color: var(--gold-bright);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .badge.ace {
    color: #c4b1ef;
    box-shadow: inset 0 0 0 1px rgba(196, 154, 240, 0.4);
  }
  .items {
    display: flex;
    gap: 1px;
    margin-top: 3px;
  }
  .slot {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    background: var(--inset);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .items :global(.role-item) {
    margin-left: 3px;
  }
  .pstats {
    flex: none;
    width: 58px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    font-variant-numeric: tabular-nums;
  }
  .pkda {
    font-size: 11.5px;
    color: var(--faint);
    white-space: nowrap;
  }
  .pkda b {
    color: var(--text-2);
    font-weight: 600;
  }
  .pkda b.d {
    color: var(--bad);
  }
  .pval {
    font-size: 10.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .pbar {
    width: 100%;
    height: 2px;
    border-radius: 1px;
    background: var(--inset);
    overflow: hidden;
  }
  .pbar span {
    display: block;
    height: 100%;
    background: var(--team);
    opacity: 0.85;
  }
  .op {
    flex: none;
    width: 28px;
    padding: 2px 0;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 700;
    text-align: center;
    color: var(--text-2);
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font-variant-numeric: tabular-nums;
  }
  .op.top {
    color: var(--gold-bright);
    box-shadow: inset 0 0 0 1px var(--gold-line);
  }
  .foot {
    margin: 0;
    padding: 0 4px;
    font-size: 11px;
    color: var(--faint);
    text-align: center;
  }
</style>
