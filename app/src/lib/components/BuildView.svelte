<!-- The imported build: source, part status, runes, spells, item path, skill order. -->
<script lang="ts">
  import { itemIcon, itemName, rune, spell } from "$lib/assets";
  import type { GameAssets, ImportReport, ItemBlock, PartResult } from "$lib/types";
  import Img from "./Img.svelte";

  let { imp, assets }: { imp: ImportReport; assets: GameAssets | null } = $props();

  const KEYS = ["Q", "W", "E", "R"] as const;

  /** "OP.GG (Gold+, flex) · Mid · patch 16.20" -> { name, tier, patch } */
  function parseSource(src: string) {
    const parts = src.split(" · ");
    const m = parts[0].match(/^(.*?)\s*\((.*)\)$/);
    const patch = parts.find((p) => p.startsWith("patch "))?.slice(6) ?? null;
    if (parts[0].startsWith("Client")) return { name: "Client recs", tier: null, patch };
    return { name: m ? m[1] : parts[0], tier: m ? m[2] : null, patch };
  }

  let sources = $derived.by(() => {
    const seen = new Map<string, ReturnType<typeof parseSource>>();
    for (const p of [imp.runes, imp.spells, imp.items]) {
      if (p.source && !seen.has(p.source)) seen.set(p.source, parseSource(p.source));
    }
    return [...seen.values()];
  });

  const statusIcon: Record<PartResult["status"], string> = {
    applied: "✓",
    off: "–",
    unavailable: "?",
    failed: "!",
  };
  const statusText: Record<PartResult["status"], string> = {
    applied: "Applied",
    off: "Off",
    unavailable: "Not available",
    failed: "Failed",
  };
  let parts = $derived([
    ["Runes", imp.runes],
    ["Spells", imp.spells],
    ["Items", imp.items],
  ] as [string, PartResult][]);

  function blockLabel(title: string): string {
    if (title.startsWith("Core")) return "Core";
    const m = title.match(/^(\d+(?:st|nd|rd|th)) item/);
    if (m) return m[1];
    if (title.startsWith("Starter")) return "Start";
    return title.length > 14 ? title.slice(0, 13) + "…" : title;
  }

  let blocks = $derived.by(() => {
    const start: ItemBlock[] = [];
    let core: ItemBlock | null = null;
    const later: ItemBlock[] = [];
    const other: ItemBlock[] = [];
    for (const b of imp.item_blocks) {
      if (b.title.startsWith("Starter") || b.title === "Boots") start.push(b);
      else if (b.title.startsWith("Core") && !core) core = b;
      else if (/^\d+(st|nd|rd|th) item/.test(b.title)) later.push(b);
      else other.push(b);
    }
    return { start, core, later, other };
  });

  let page = $derived(imp.rune_page);
  let keystone = $derived(page ? rune(assets, page.perks[0]) : null);
  let primaryTree = $derived(page ? rune(assets, page.primary_style) : null);
  let secondaryTree = $derived(page ? rune(assets, page.sub_style) : null);
  let priority = $derived(imp.skill_priority?.split(" > ") ?? []);
  let levels = $derived(imp.skill_order.slice(0, 18));

  function pct(x: number) {
    return `${(x * 100).toFixed(1)}%`;
  }
  function compact(n: number) {
    return n >= 10000 ? `${Math.round(n / 1000)}k` : n.toLocaleString();
  }
</script>

<div class="build">
  <div class="meta">
    <div class="sources">
      {#each sources as s}
        <span class="source" title="Build source">
          <b>{s.name}</b>
          {#if s.tier}<span>{s.tier}</span>{/if}
          {#if s.patch}<span class="patch">{s.patch}</span>{/if}
        </span>
      {:else}
        <span class="source"><b>No build data</b></span>
      {/each}
    </div>
    {#if imp.games}
      <div class="stats">
        {#if imp.win_rate}<span class="wr" class:good={imp.win_rate >= 0.5}>{pct(imp.win_rate)} WR</span>{/if}
        <span class="games">{compact(imp.games)} games</span>
      </div>
    {/if}
  </div>

  <ul class="parts" aria-label="Import result">
    {#each parts as [name, p]}
      <li class="part {p.status}" title="{name}: {statusText[p.status]}{p.status === 'applied' ? '' : ` — ${p.detail}`}">
        <span class="pi" aria-hidden="true">{statusIcon[p.status]}</span>{name}
        <span class="sr">{statusText[p.status]}</span>
      </li>
    {/each}
  </ul>

  {#if page || imp.spell_ids}
    <section class="runes">
      {#if page}
        <span class="keystone">
          <Img src={keystone?.icon} alt={keystone?.name ?? "Keystone"} size={46} shape="round" />
        </span>
        <div class="rune-detail">
          <div class="rune-title">
            <strong>{keystone?.name ?? "Keystone"}</strong>
            <span class="trees">
              <Img src={primaryTree?.icon} alt={primaryTree?.name ?? "Primary tree"} size={14} shape="bare" />
              {primaryTree?.name ?? "Primary"}
              <span class="plus">+</span>
              <Img src={secondaryTree?.icon} alt={secondaryTree?.name ?? "Secondary tree"} size={14} shape="bare" />
              {secondaryTree?.name ?? "Secondary"}
            </span>
          </div>
          <div class="rune-row">
            {#each page.perks.slice(1, 4) as id}
              {@const r = rune(assets, id)}
              <Img src={r?.icon} alt={r?.name ?? `Rune ${id}`} size={24} shape="round" class="minor" />
            {/each}
            <span class="sep" aria-hidden="true"></span>
            {#each page.perks.slice(4, 6) as id}
              {@const r = rune(assets, id)}
              <Img src={r?.icon} alt={r?.name ?? `Rune ${id}`} size={24} shape="round" class="minor" />
            {/each}
            <span class="sep" aria-hidden="true"></span>
            {#each page.perks.slice(6, 9) as id}
              {@const r = rune(assets, id)}
              <Img src={r?.icon} alt={r?.name ?? `Shard ${id}`} size={16} shape="round" class="shard" />
            {/each}
          </div>
        </div>
      {/if}
      {#if imp.spell_ids}
        <div class="spells" aria-label="Summoner spells">
          {#each imp.spell_ids as id, i}
            {@const sp = spell(assets, id)}
            <span class="spell">
              <Img src={sp?.icon} alt={sp?.name ?? `Spell ${id}`} size={30} shape="rounded" />
              <kbd>{i === 0 ? "D" : "F"}</kbd>
            </span>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  {#if imp.item_blocks.length}
    <section class="items" aria-label="Item path">
      <div class="item-groups">
        {#each blocks.start as b}
          <div class="group">
            <span class="glabel">{blockLabel(b.title)}</span>
            <div class="icons">
              {#each b.items as [id, count]}
                <span class="item">
                  <Img src={itemIcon(assets, id)} alt={itemName(assets, id)} size={28} />
                  {#if count > 1}<span class="count">{count}</span>{/if}
                </span>
              {/each}
            </div>
          </div>
        {/each}
        {#if blocks.core}
          <div class="group core">
            <span class="glabel">Core</span>
            <div class="icons">
              {#each blocks.core.items as [id], i}
                {#if i > 0}<span class="arrow" aria-hidden="true">›</span>{/if}
                <span class="item">
                  <Img src={itemIcon(assets, id)} alt={itemName(assets, id)} size={28} />
                </span>
              {/each}
            </div>
          </div>
        {/if}
      </div>
      {#if blocks.later.length || blocks.other.length}
        <div class="item-groups later">
          {#each [...blocks.later, ...blocks.other] as b}
            <div class="group">
              <span class="glabel">{blockLabel(b.title)}</span>
              <div class="icons">
                {#each b.items.slice(0, 3) as [id]}
                  <span class="item opt">
                    <Img src={itemIcon(assets, id)} alt={itemName(assets, id)} size={24} />
                  </span>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  {#if levels.length || priority.length}
    <section class="skills" aria-label="Skill order">
      {#if priority.length}
        <div class="priority">
          <span class="glabel">Max</span>
          {#each priority as k, i}
            {#if i > 0}<span class="arrow" aria-hidden="true">›</span>{/if}
            <span class="key key-{k}">{k}</span>
          {/each}
        </div>
      {/if}
      {#if levels.length}
        <div class="grid" style:--n={levels.length} role="table" aria-label="Skill per level">
          {#each KEYS as k}
            <span class="key key-{k} rowkey" role="rowheader">{k}</span>
            {#each levels as s, lvl}
              <span class="cell key-{k}" class:on={s === k} role="cell">
                {#if s === k}{lvl + 1}{/if}
              </span>
            {/each}
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .build {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sources {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .source {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border-radius: 3px;
    font-size: 11px;
    color: var(--text-2);
    background: var(--teal-wash);
    box-shadow: inset 0 0 0 1px var(--teal-line);
  }
  .source b {
    color: var(--teal-bright);
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  .source .patch {
    color: var(--muted);
  }
  .source .patch::before {
    content: "v";
  }
  .stats {
    display: flex;
    gap: 8px;
    font-size: 11.5px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .wr {
    color: var(--text-2);
    font-weight: 600;
  }
  .wr.good {
    color: var(--ok);
  }

  .parts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    gap: 6px;
  }
  .part {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 4px 6px;
    border-radius: 3px;
    font-size: 11.5px;
    color: var(--muted);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    cursor: default;
  }
  .pi {
    font-weight: 700;
    font-size: 11px;
  }
  .part.applied {
    color: var(--text-2);
  }
  .part.applied .pi {
    color: var(--ok);
  }
  .part.failed {
    color: var(--bad);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--bad) 45%, transparent);
  }
  .part.unavailable {
    color: var(--warn);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  /* runes + spells */
  .runes {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .keystone {
    flex: none;
    padding: 3px;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 35%, #1c2a3b, #0a0f17 70%);
    box-shadow:
      0 0 0 1px var(--gold-deep),
      0 0 14px -4px var(--gold-glow);
    display: grid;
  }
  .rune-detail {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rune-title {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .rune-title strong {
    font-size: 13.5px;
    font-weight: 650;
    color: var(--gold-bright);
  }
  .trees {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
  }
  .plus {
    color: var(--faint);
    margin: 0 1px;
  }
  .rune-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .rune-row :global(.minor) {
    background: #0a0f17;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .rune-row :global(.shard) {
    background: #0a0f17;
    box-shadow: inset 0 0 0 1px var(--line);
    padding: 1px;
  }
  .sep {
    width: 1px;
    height: 16px;
    margin: 0 3px;
    background: var(--line-strong);
  }
  .spells {
    display: flex;
    gap: 6px;
    flex: none;
    align-self: flex-start;
  }
  .spell {
    position: relative;
    display: grid;
    box-shadow: 0 0 0 1px var(--line-strong);
    border-radius: 6px;
  }
  kbd {
    position: absolute;
    right: -4px;
    bottom: -5px;
    font: 600 9px/1 var(--font);
    padding: 2px 3px;
    border-radius: 2px;
    background: var(--bg);
    color: var(--muted);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  /* items */
  .items {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px;
    border-radius: 4px;
    background: var(--inset);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .item-groups {
    display: flex;
    gap: 8px 14px;
    flex-wrap: wrap;
  }
  .core {
    margin-left: auto;
  }
  .later {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 10px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .glabel {
    font-size: 10px;
    font-weight: 650;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--faint);
  }
  .core .glabel {
    color: var(--gold);
  }
  .icons {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .item {
    position: relative;
    display: grid;
    border-radius: 3px;
    box-shadow: 0 0 0 1px var(--line-strong);
    transition: transform 0.12s;
  }
  .core .item {
    box-shadow:
      0 0 0 1px var(--gold-deep),
      0 2px 8px -2px rgba(0, 0, 0, 0.6);
  }
  .item:hover {
    transform: translateY(-1px);
  }
  .opt {
    opacity: 0.92;
  }
  .count {
    position: absolute;
    right: 1px;
    bottom: 0;
    font-size: 10px;
    font-weight: 700;
    color: #fff;
    text-shadow:
      0 0 2px #000,
      0 0 2px #000;
  }
  .arrow {
    color: var(--gold-dim);
    font-size: 15px;
    line-height: 1;
    padding: 0 1px;
  }

  /* skills */
  .skills {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .priority {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .priority .glabel {
    margin-right: 3px;
  }
  .key {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 700;
    color: var(--k);
    background: color-mix(in srgb, var(--k) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 40%, transparent);
  }
  .key-Q {
    --k: var(--skill-q);
  }
  .key-W {
    --k: var(--skill-w);
  }
  .key-E {
    --k: var(--skill-e);
  }
  .key-R {
    --k: var(--gold);
  }
  .grid {
    display: grid;
    grid-template-columns: 18px repeat(var(--n), minmax(0, 1fr));
    gap: 2px;
    max-width: 100%;
  }
  .rowkey {
    width: 18px;
    height: 16px;
    font-size: 10px;
  }
  .cell {
    height: 16px;
    border-radius: 2px;
    background: var(--inset);
    display: grid;
    place-items: center;
    font-size: 9.5px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    color: transparent;
  }
  .cell.on {
    color: #0b1119;
    background: var(--k);
  }
</style>
