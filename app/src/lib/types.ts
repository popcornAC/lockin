// Mirrors the Rust structs serialized by the backend (src-tauri/src/service.rs etc).

export interface Settings {
  auto_accept: boolean;
  auto_import: boolean;
  import_runes: boolean;
  import_spells: boolean;
  import_items: boolean;
  use_opgg: boolean;
  opgg_tier: string;
  player_lookup: boolean;
  detect_premades: boolean;
  launch_at_login: boolean;
}

export interface ChampSelectView {
  champion_id: number;
  champion_name: string;
  position: string;
  locked: boolean;
  map_id: number | null;
  queue_id: number | null;
  supported: boolean;
  message: string | null;
}

export type PartStatus = "applied" | "off" | "unavailable" | "failed";

export interface PartResult {
  status: PartStatus;
  source: string | null;
  detail: string;
}

export interface ImportReport {
  champion_id: number;
  champion_name: string;
  position: string;
  runes: PartResult;
  spells: PartResult;
  items: PartResult;
  skill_priority: string | null;
  games: number | null;
  win_rate: number | null;
  rune_page: RunePage | null;
  /** In slot order (D, F) when set in champ select. */
  spell_ids: [number, number] | null;
  item_blocks: ItemBlock[];
  /** Level-by-level skill order ("Q", "W", ...); empty if the source has none. */
  skill_order: string[];
  notes: string[];
}

export interface RunePage {
  primary_style: number;
  sub_style: number;
  /** keystone, 3 primary, 2 secondary, 3 stat shards */
  perks: number[];
}

export interface ItemBlock {
  title: string;
  /** [item id, count] */
  items: [number, number][];
}

export interface Named {
  name: string;
  icon: string;
}

/** Icon lookup tables (src-tauri/src/assets.rs). */
export interface GameAssets {
  version: string;
  /** numeric champion id / normalised name / normalised id -> Data Dragon id */
  champions: Record<string, string>;
  champion_names: Record<string, string>;
  spells: Record<string, Named>;
  runes: Record<string, Named>;
  items: Record<string, string>;
}

export interface Rank {
  tier: string;
  division: number | null;
  lp: number | null;
  wins: number;
  losses: number;
}

export interface ChampStat {
  name: string;
  games: number;
  wins: number;
}

export interface Profile {
  level: number | null;
  solo: Rank | null;
  flex: Rank | null;
  top_champions: ChampStat[];
}

/** src-tauri/src/premades.rs */
export interface PartyTag {
  /** 0 = your own party; inferred parties count from 1 (used for the colour). */
  id: number;
  size: number;
  /** Inferred from shared recent games on OP.GG, not known. */
  likely: boolean;
  games: number | null;
}

export interface PlayerView {
  riot_id: string;
  champion: string | null;
  team: string | null;
  is_self: boolean;
  profile: Profile | null;
  error: string | null;
  loading: boolean;
  party: PartyTag | null;
}

export interface Status {
  connection: "searching" | "connected";
  tls: "pinned_riot_root" | "unverified_localhost" | null;
  creds_source: string | null;
  phase: string;
  patch: string | null;
  region: string | null;
  champ_select: ChampSelectView | null;
  last_import: ImportReport | null;
  importing: boolean;
  players_context: "" | "champ_select" | "in_game";
  players: PlayerView[];
  log: string[];
}

/** src-tauri/src/opgg/games.rs: one row of the Games list. */
export interface GameSummary {
  id: string;
  /** Unix seconds when the game ended. */
  ended_at: number;
  duration: number;
  /** OP.GG game type, e.g. "SOLORANKED". */
  queue: string;
  win: boolean | null;
  champion_id: number;
  champion: string;
  position: string;
  kills: number;
  deaths: number;
  assists: number;
  cs: number;
}

/** A finished game OP.GG doesn't have yet (src-tauri/src/games.rs). */
export interface PendingGame {
  lcu_game_id: number;
  ended_at: number;
  queue_id: number | null;
  champion_id: number | null;
}

export interface GamesView {
  /** Newest first. */
  games: GameSummary[];
  pending: PendingGame[];
  syncing: boolean;
  error: string | null;
  /** "Name#TAG" whose games these are. */
  account: string | null;
}

export interface TeamRecord {
  side: "BLUE" | "RED" | string;
  win: boolean;
  kills: number;
  dragons: number;
  barons: number;
  towers: number;
  gold: number;
  bans: number[];
}

export interface PlayerRecord {
  riot_id: string;
  side: string;
  /** LCU spelling: top, jungle, middle, bottom, utility. */
  position: string;
  champion_id: number;
  champion: string;
  level: number;
  kills: number;
  deaths: number;
  assists: number;
  cs: number;
  gold: number;
  damage: number;
  damage_taken: number;
  wards_placed: number;
  control_wards: number;
  largest_multi_kill: number;
  items: number[];
  role_item: number | null;
  spells: number[];
  primary_style: number | null;
  keystone: number | null;
  sub_style: number | null;
  /** OP.GG's 0-10 score and its rank in the game (1 = best). */
  op_score: number | null;
  op_rank: number | null;
}

/** One stored game: the whole scoreboard. */
export interface GameRecord {
  version: number;
  id: string;
  region: string;
  ended_at: number;
  duration: number;
  queue: string;
  average_tier: string | null;
  /** Blue then red. */
  teams: TeamRecord[];
  /** Blue then red, each top to support. */
  players: PlayerRecord[];
  me: number | null;
}
