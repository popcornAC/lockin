// Image URL helpers. Lookup tables come from the backend (GameAssets); every
// helper returns null when it can't resolve a URL so the caller shows a placeholder.

import type { GameAssets } from "./types";

const DDRAGON = "https://ddragon.leagueoflegends.com/cdn";
const CDRAGON = "https://raw.communitydragon.org/latest/plugins";

/** Same normalisation as the backend: lower-case alphanumerics only. */
export function norm(s: string): string {
  return s.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
}

/** Data Dragon champion id ("MonkeyKing") from a numeric id or any spelling of the name. */
export function championKey(a: GameAssets | null, champ: number | string | null | undefined): string | null {
  if (!a || champ === null || champ === undefined || champ === "" || champ === 0) return null;
  const k = typeof champ === "number" ? String(champ) : norm(champ);
  return a.champions[k] ?? null;
}

export function championIcon(a: GameAssets | null, champ: number | string | null | undefined): string | null {
  const key = championKey(a, champ);
  return key && a ? `${DDRAGON}/${a.version}/img/champion/${key}.png` : null;
}

/** Wide, centred splash art (unversioned on Data Dragon). */
export function championSplash(a: GameAssets | null, champ: number | string | null | undefined): string | null {
  const key = championKey(a, champ);
  return key ? `${DDRAGON}/img/champion/centered/${key}_0.jpg` : null;
}

export function itemIcon(a: GameAssets | null, id: number): string | null {
  return a ? `${DDRAGON}/${a.version}/img/item/${id}.png` : null;
}

export function itemName(a: GameAssets | null, id: number): string {
  return a?.items[id] ?? `Item ${id}`;
}

export function spell(a: GameAssets | null, id: number) {
  return a?.spells[id] ?? null;
}

export function rune(a: GameAssets | null, id: number) {
  return a?.runes[id] ?? null;
}

const TIERS = [
  "iron",
  "bronze",
  "silver",
  "gold",
  "platinum",
  "emerald",
  "diamond",
  "master",
  "grandmaster",
  "challenger",
];

/** Small ranked crest from CommunityDragon. */
export function rankCrest(tier: string | null | undefined): string {
  const t = (tier ?? "").toLowerCase();
  const name = TIERS.includes(t) ? t : "unranked";
  return `${CDRAGON}/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/${name}.svg`;
}

const POSITIONS = ["top", "jungle", "middle", "bottom", "utility"];

export function positionIcon(pos: string | null | undefined): string | null {
  return pos && POSITIONS.includes(pos)
    ? `${CDRAGON}/rcp-fe-lol-clash/global/default/assets/images/position-selector/positions/icon-position-${pos}.png`
    : null;
}

export const POSITION_LABELS: Record<string, string> = {
  top: "Top",
  jungle: "Jungle",
  middle: "Mid",
  bottom: "ADC",
  utility: "Support",
};
