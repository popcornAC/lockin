// Dev-only mock backend (see backend.ts). Pick a state with `?mock=<state>`:
// disconnected | idle | lobby | queue | hover | champselect | importing |
// unsupported | ingame. Icon tables are built from live Data Dragon data.

import type { Backend } from "./backend";
import type { GameAssets, ImportReport, PartyTag, PlayerView, Profile, Rank, Settings, Status } from "./types";

const TIERS: [string, string][] = [
  ["all", "All ranks"],
  ["gold_plus", "Gold+"],
  ["platinum_plus", "Platinum+"],
  ["emerald_plus", "Emerald+"],
  ["diamond_plus", "Diamond+"],
  ["master_plus", "Master+"],
  ["master", "Master"],
  ["grandmaster", "Grandmaster"],
  ["challenger", "Challenger"],
];

const settings: Settings = {
  auto_accept: true,
  auto_import: true,
  import_runes: true,
  import_spells: true,
  import_items: true,
  use_opgg: true,
  opgg_tier: "gold_plus",
  player_lookup: true,
  detect_premades: true,
  launch_at_login: false,
};

const LOG = [
  "18:42:07 Items: Applied from OP.GG (Gold+) · Mid · patch 16.20 - item set \"Ahri Mid (Lockin)\"",
  "18:42:07 Spells: Applied from OP.GG (Gold+) · Mid · patch 16.20 - spells 4 + 14",
  "18:42:06 Runes: Applied from OP.GG (Gold+) · Mid · patch 16.20 - rune page \"[Lockin] Ahri Mid\"",
  "18:42:05 Importing build for Ahri (Mid)",
  "18:41:12 Ready check accepted",
  "18:38:40 Connected to League client on port 51234 (certificate verified against Riot root)",
];

const opgg = "OP.GG (Gold+) · Mid · patch 16.20";
const ahriImport: ImportReport = {
  champion_id: 103,
  champion_name: "Ahri",
  position: "middle",
  runes: { status: "applied", source: opgg, detail: 'rune page "[Lockin] Ahri Mid"' },
  spells: { status: "applied", source: opgg, detail: "spells 4 + 14" },
  items: { status: "applied", source: opgg, detail: 'item set "Ahri Mid (Lockin)"' },
  skill_priority: "Q > W > E",
  games: 225190,
  win_rate: 114145 / 225190,
  rune_page: { primary_style: 8100, sub_style: 8200, perks: [8112, 8139, 8140, 8106, 8210, 8226, 5005, 5008, 5001] },
  spell_ids: [4, 14],
  item_blocks: [
    { title: "Starter", items: [[1056, 1], [2003, 2]] },
    { title: "Boots", items: [[3020, 1]] },
    { title: "Core · skills Q > W > E (start W-Q-E)", items: [[3118, 1], [4645, 1], [3157, 1]] },
    { title: "4th item options", items: [[3089, 1], [3157, 1], [4645, 1]] },
    { title: "5th item options", items: [[3089, 1], [3135, 1], [3157, 1]] },
    { title: "6th item options", items: [[4629, 1], [3135, 1], [4646, 1]] },
  ],
  skill_order: "W Q E Q Q R Q W Q W R W W E E".split(" "),
  notes: [],
};

function rank(tier: string, division: number | null, lp: number, wins: number, losses: number): Rank {
  return { tier, division, lp, wins, losses };
}

function profile(solo: Rank | null, champs: [string, number, number][]): Profile {
  return {
    level: 200 + champs.length * 37,
    solo,
    flex: null,
    top_champions: champs.map(([name, games, wins]) => ({ name, games, wins })),
  };
}

function player(riot_id: string, champion: string | null, team: string | null, p: Profile | null, extra: Partial<PlayerView> = {}): PlayerView {
  return { riot_id, champion, team, is_self: false, profile: p, error: null, loading: false, party: null, ...extra };
}

/** Your own party (known from the lobby) and inferred ones (in game only). */
const ownDuo: PartyTag = { id: 0, size: 2, likely: false, games: null };
const likely = (id: number, size: number, games: number): PartyTag => ({ id, size, likely: true, games });

const me = () =>
  player("Arik#EUW", "Ahri", "ORDER", profile(rank("EMERALD", 2, 61, 118, 97), [["Ahri", 88, 51], ["Syndra", 41, 22], ["Orianna", 19, 9]]), {
    is_self: true,
  });

const inGame: PlayerView[] = [
  player("Thornwood#EUW", "Garen", "ORDER", profile(rank("PLATINUM", 1, 12, 64, 59), [["Garen", 70, 40], ["Darius", 22, 11], ["Sett", 15, 7]]), {
    party: likely(1, 2, 4),
  }),
  player("Lantern Moth#2207", "Lee Sin", "ORDER", profile(rank("EMERALD", 4, 33, 90, 88), [["Lee Sin", 102, 54], ["Viego", 31, 15], ["Kha'Zix", 12, 5]]), {
    party: likely(1, 2, 4),
  }),
  { ...me(), party: ownDuo },
  player("quietstorm#0001", "Jinx", "ORDER", profile(rank("DIAMOND", 3, 75, 140, 121), [["Jinx", 131, 74], ["Kai'Sa", 60, 29], ["Ezreal", 18, 9]]), {
    party: ownDuo,
  }),
  player("Pebble#EUW", "Thresh", "ORDER", null, { loading: true }),
  player("Vortex Seven#777", "K'Sante", "CHAOS", profile(rank("EMERALD", 1, 88, 77, 60), [["K'Sante", 51, 29], ["Jax", 20, 12], ["Gnar", 9, 4]])),
  player("NightOwl#4413", "Vi", "CHAOS", profile(rank("GOLD", 2, 40, 45, 52), [["Vi", 39, 18], ["Warwick", 16, 9], ["Amumu", 10, 6]]), {
    party: likely(2, 3, 6),
  }),
  player("Sora#KR1", "Syndra", "CHAOS", profile(rank("MASTER", null, 214, 201, 168), [["Syndra", 122, 70], ["Azir", 49, 26], ["Taliyah", 22, 10]]), {
    party: likely(2, 3, 6),
  }),
  player("bluefinch#EUW", "Miss Fortune", "CHAOS", profile(null, [["Miss Fortune", 14, 8]]), { party: likely(2, 3, 3) }),
  player("Ghostlight#9001", "Nautilus", "CHAOS", null, { error: "Summoner not found on OP.GG" }),
];

function status(state: string): Status {
  const base: Status = {
    connection: "connected",
    tls: "pinned_riot_root",
    creds_source: "process",
    phase: "None",
    patch: "16.20",
    region: "EUW",
    champ_select: null,
    last_import: null,
    importing: false,
    players_context: "",
    players: [],
    log: LOG,
  };
  const cs = { champion_id: 103, champion_name: "Ahri", position: "middle", locked: true, map_id: 11, queue_id: 420, supported: true, message: null };
  switch (state) {
    case "disconnected":
      return { ...base, connection: "searching", tls: null, creds_source: null, region: null, log: LOG.slice(5) };
    case "lobby":
      return { ...base, phase: "Lobby", last_import: ahriImport };
    case "queue":
      return { ...base, phase: "Matchmaking" };
    case "hover":
      return {
        ...base,
        phase: "ChampSelect",
        champ_select: { ...cs, locked: false, champion_id: 238, champion_name: "Zed" },
        players_context: "champ_select",
        players: [me()],
      };
    case "importing":
      return { ...base, phase: "ChampSelect", champ_select: cs, importing: true, players_context: "champ_select", players: [me()] };
    case "champselect":
      return {
        ...base,
        phase: "ChampSelect",
        champ_select: cs,
        last_import: ahriImport,
        players_context: "champ_select",
        players: [{ ...me(), party: ownDuo }, inGame[3]],
      };
    case "unsupported":
      return {
        ...base,
        phase: "ChampSelect",
        champ_select: { ...cs, champion_id: 0, champion_name: "", position: "", locked: false, map_id: 12, queue_id: 450, supported: false, message: "Not supported in this mode (Summoner's Rift only)" },
      };
    case "ingame":
      return { ...base, phase: "InProgress", last_import: ahriImport, players_context: "in_game", players: inGame };
    default:
      return base;
  }
}

/** Build GameAssets the same way src-tauri/src/assets.rs does, from live Data Dragon. */
async function liveAssets(): Promise<GameAssets | null> {
  const get = (u: string) => fetch(u).then((r) => (r.ok ? r.json() : Promise.reject(r.status)));
  try {
    const dd = "https://ddragon.leagueoflegends.com";
    const cd = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1";
    const version: string = (await get(`${dd}/api/versions.json`))[0];
    const data = `${dd}/cdn/${version}/data/en_US`;
    const [champs, spells, runes, items, perks] = await Promise.all([
      get(`${data}/champion.json`),
      get(`${data}/summoner.json`),
      get(`${data}/runesReforged.json`),
      get(`${data}/item.json`),
      get(`${cd}/perks.json`).catch(() => []),
    ]);
    const n = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
    const a: GameAssets = { version, champions: {}, champion_names: {}, spells: {}, runes: {}, items: {} };
    for (const [id, c] of Object.entries<any>(champs.data)) {
      a.champions[c.key] = id;
      a.champions[n(id)] = id;
      a.champions[n(c.name)] = id;
      a.champion_names[id] = c.name;
    }
    for (const s of Object.values<any>(spells.data)) {
      a.spells[s.key] = { name: s.name, icon: `${dd}/cdn/${version}/img/spell/${s.image.full}` };
    }
    for (const st of runes) {
      a.runes[st.id] = { name: st.name, icon: `${dd}/cdn/img/${st.icon}` };
      for (const sl of st.slots) for (const r of sl.runes) a.runes[r.id] = { name: r.name, icon: `${dd}/cdn/img/${r.icon}` };
    }
    for (const p of perks as any[]) {
      if (p.id >= 5000 && p.id < 6000 && p.iconPath?.startsWith("/lol-game-data/assets/v1/")) {
        a.runes[p.id] = { name: p.name, icon: `${cd}/${p.iconPath.slice(25).toLowerCase()}` };
      }
    }
    for (const [id, it] of Object.entries<any>(items.data)) a.items[id] = it.name;
    return a;
  } catch {
    return null; // offline: placeholders everywhere
  }
}

export function mockBackend(): Backend {
  const q = new URLSearchParams(location.search);
  const state = q.get("mock") ?? "champselect";
  let s = { ...settings };
  const assets = q.has("offline") ? Promise.resolve(null) : liveAssets();
  const statusCbs: ((s: Status) => void)[] = [];
  return {
    getStatus: async () => status(state),
    getSettings: async () => s,
    setSettings: async (n) => (s = { ...n }),
    tiers: async () => TIERS,
    getAssets: () => assets,
    applyNow: async () => {
      const st = status(state);
      statusCbs.forEach((cb) => cb({ ...st, importing: true }));
      await new Promise((r) => setTimeout(r, 900));
      statusCbs.forEach((cb) => cb({ ...st, importing: false, last_import: ahriImport }));
    },
    refreshBuilds: async () => {},
    on: async (event: string, cb: (p: never) => void) => {
      if (event === "status") statusCbs.push(cb as (s: Status) => void);
      return () => {};
    },
  };
}
