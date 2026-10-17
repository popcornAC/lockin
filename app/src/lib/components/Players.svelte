<!-- Player lookup results: one tidy row per player, grouped by team in game.
     Party chips: gold for your own party (known from the client), other colours
     for parties inferred from match history, which are always labelled "likely". -->
<script lang="ts">
  import { championIcon, rankCrest } from "$lib/assets";
  import type { GameAssets, PartyTag, PlayerView, Rank } from "$lib/types";
  import Img from "./Img.svelte";

  let { players, inGame, assets }: { players: PlayerView[]; inGame: boolean; assets: GameAssets | null } = $props();

  let teams = $derived.by(() => {
    if (!inGame) return [{ id: "", name: "", list: players }];
    return [
      { id: "blue", name: "Blue side", list: players.filter((p) => p.team === "ORDER") },
      { id: "red", name: "Red side", list: players.filter((p) => p.team === "CHAOS") },
    ].filter((t) => t.list.length);
  });

  const APEX = ["MASTER", "GRANDMASTER", "CHALLENGER"];
  const ROMAN = ["", "I", "II", "III", "IV"];

  function tierText(r: Rank): string {
    const t = r.tier.charAt(0) + r.tier.slice(1).toLowerCase();
    return !APEX.includes(r.tier) && r.division ? `${t} ${ROMAN[r.division] ?? r.division}` : t;
  }

  function winRate(wins: number, games: number) {
    return games > 0 ? Math.round((100 * wins) / games) : null;
  }

  const SIZES = ["", "", "Duo", "Trio", "4-stack", "5-stack"];
  const partySize = (t: PartyTag) => SIZES[t.size] ?? `${t.size}-stack`;

  function partyTitle(t: PartyTag): string {
    if (!t.likely) return `Your party (${t.size}), from the League client`;
    const g = t.games ? `${t.games} recent games together` : "recent games together";
    return `Likely ${partySize(t).toLowerCase()}: ${g} on the same side (OP.GG match history). Inferred, not confirmed.`;
  }

  let anyLikely = $derived(players.some((p) => p.party?.likely));

  function splitId(id: string): [string, string] {
    const i = id.lastIndexOf("#");
    return i < 0 ? [id, ""] : [id.slice(0, i), id.slice(i)];
  }
</script>

{#each teams as t (t.id)}
  <div class="team {t.id}">
    {#if t.name}
      <div class="team-head"><span class="bar"></span>{t.name}</div>
    {/if}
    <ul>
      {#each t.list as p (p.riot_id)}
        {@const [name, tag] = splitId(p.riot_id)}
        {@const solo = p.profile?.solo ?? null}
        {@const games = solo ? solo.wins + solo.losses : 0}
        {@const wr = solo ? winRate(solo.wins, games) : null}
        <li class="player" class:me={p.is_self}>
          {#if p.champion}
            <Img src={championIcon(assets, p.champion)} alt={p.champion} size={34} class="champ" initials />
          {:else}
            <Img src={solo ? rankCrest(solo.tier) : rankCrest(null)} alt="Rank" size={34} shape="bare" />
          {/if}
          <div class="who">
            <div class="name">
              <span class="gn">{name}</span><span class="tag">{tag}</span>
              {#if p.is_self}<span class="you">You</span>{/if}
              {#if p.party}
                <span class="party p{Math.min(p.party.id, 4)}" class:likely={p.party.likely} title={partyTitle(p.party)}>
                  <svg viewBox="0 0 12 8" aria-hidden="true"
                    ><circle cx="4" cy="4" r="2.6" /><circle cx="8" cy="4" r="2.6" /></svg
                  >{p.party.likely ? "Likely " : ""}{partySize(p.party)}
                </span>
              {/if}
            </div>
            <div class="rank">
              {#if p.loading}
                <span class="skeleton" aria-label="Loading"></span>
              {:else if p.error}
                <span class="err">{p.error.includes("not found") ? "Not found on OP.GG" : "Lookup failed"}</span>
              {:else if solo}
                {#if p.champion}<Img src={rankCrest(solo.tier)} alt={tierText(solo)} size={16} shape="bare" />{/if}
                <span class="tier tier-{solo.tier.toLowerCase()}">{tierText(solo)}</span>
                <span class="lp">{solo.lp ?? 0} LP</span>
                {#if wr !== null}
                  <span class="dot">·</span>
                  <span class="wr" class:good={wr >= 52} class:bad={wr < 48}>{wr}%</span>
                  <span class="games">{games}g</span>
                {/if}
              {:else if p.profile}
                <span class="unranked">Unranked{p.profile.level ? ` · lvl ${p.profile.level}` : ""}</span>
              {/if}
            </div>
          </div>
          {#if p.profile?.top_champions.length}
            <div class="mains" aria-label="Most played">
              {#each p.profile.top_champions as c}
                {@const cwr = winRate(c.wins, c.games)}
                <span class="main" title="{c.name}: {c.games} games{cwr !== null ? `, ${cwr}% win rate` : ''}">
                  <Img src={championIcon(assets, c.name)} alt={c.name} size={22} shape="round" initials />
                  <span class="mwr" class:good={cwr !== null && cwr >= 52} class:bad={cwr !== null && cwr < 48}>{cwr ?? "–"}</span>
                </span>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
{/each}
{#if anyLikely}
  <p class="legend">"Likely" parties are guessed from recent games played together (OP.GG), not confirmed.</p>
{/if}

<style>
  .team + .team {
    margin-top: 10px;
  }
  .team-head {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 0 6px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .bar {
    width: 3px;
    height: 11px;
    border-radius: 1px;
    background: var(--team);
  }
  .blue {
    --team: var(--blue);
  }
  .red {
    --team: var(--red);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .player {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--surface-2);
    box-shadow: inset 2px 0 0 var(--team, var(--line-strong));
    transition: background-color 0.15s;
  }
  .player:hover {
    background: var(--surface-3);
  }
  .player.me {
    background: linear-gradient(90deg, var(--gold-wash), var(--surface-2) 70%);
    box-shadow:
      inset 2px 0 0 var(--gold),
      inset 0 0 0 1px var(--gold-line);
  }
  .player :global(.champ) {
    box-shadow: 0 0 0 1px var(--line-strong);
  }
  .me :global(.champ) {
    box-shadow: 0 0 0 1px var(--gold);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    display: flex;
    align-items: baseline;
    gap: 1px;
    white-space: nowrap;
    overflow: hidden;
  }
  .gn {
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    color: var(--faint);
    font-size: 11.5px;
  }
  .you {
    margin-left: 6px;
    padding: 0 5px;
    border-radius: 2px;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--gold-bright);
    background: var(--gold-wash);
    box-shadow: inset 0 0 0 1px var(--gold-line);
    align-self: center;
  }
  /* Party chip. --pc is the party colour: gold = your party, others inferred. */
  .party {
    --pc: var(--gold-bright);
    --pc-wash: var(--gold-wash);
    --pc-line: var(--gold-line);
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: 5px;
    padding: 0 5px 0 4px;
    border-radius: 2px;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--pc);
    background: var(--pc-wash);
    box-shadow: inset 0 0 0 1px var(--pc-line);
    align-self: center;
    cursor: help;
  }
  .party svg {
    width: 11px;
    height: 8px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.3;
  }
  /* Inferred: dashed outline, no fill, so it never reads like a known fact. */
  .party.likely {
    background: transparent;
    box-shadow: none;
    outline: 1px dashed var(--pc-line);
    outline-offset: -1px;
  }
  .p1 {
    --pc: var(--teal-bright);
    --pc-line: rgba(127, 216, 224, 0.55);
  }
  .p2 {
    --pc: #cdb2f7;
    --pc-line: rgba(196, 154, 240, 0.6);
  }
  .p3 {
    --pc: #f2a7cf;
    --pc-line: rgba(232, 140, 190, 0.6);
  }
  .p4 {
    --pc: #b9d97a;
    --pc-line: rgba(170, 210, 100, 0.55);
  }
  .legend {
    margin: 8px 2px 0;
    font-size: 10.5px;
    color: var(--faint);
  }
  .rank {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 16px;
    font-size: 11.5px;
    color: var(--muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .tier {
    color: var(--text-2);
    font-weight: 600;
  }
  .tier-iron,
  .tier-bronze {
    color: #b88c74;
  }
  .tier-silver {
    color: #a9b4bf;
  }
  .tier-gold {
    color: #d9b45f;
  }
  .tier-platinum {
    color: #5fc4b5;
  }
  .tier-emerald {
    color: #3fc27f;
  }
  .tier-diamond {
    color: #7d9cf0;
  }
  .tier-master,
  .tier-grandmaster {
    color: #c77ae0;
  }
  .tier-challenger {
    color: #f0cf7a;
  }
  .dot {
    color: var(--faint);
  }
  .wr {
    color: var(--text-2);
    font-weight: 600;
  }
  .good {
    color: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
  .games {
    color: var(--faint);
  }
  .err,
  .unranked {
    color: var(--faint);
    font-style: italic;
  }
  .skeleton {
    display: block;
    width: 120px;
    height: 9px;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--surface-3) 0%, var(--line-strong) 50%, var(--surface-3) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.2s linear infinite;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }
  .mains {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .main {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .main :global(.img) {
    box-shadow: 0 0 0 1px var(--line-strong);
  }
  .mwr {
    font-size: 9px;
    line-height: 1;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .mwr.good {
    color: var(--ok);
  }
  .mwr.bad {
    color: var(--bad);
  }
</style>
