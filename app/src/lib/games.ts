// Formatting and derived stats for the Games tab. Everything here is computed
// from what OP.GG reports; nothing is estimated.

import type { GameRecord, PlayerRecord } from "./types";

const OPGG_QUEUES: Record<string, string> = {
  SOLORANKED: "Ranked Solo",
  FLEXRANKED: "Ranked Flex",
  NORMAL: "Normal",
  BLIND: "Blind Pick",
  QUICKPLAY: "Quickplay",
};

const CLIENT_QUEUES: Record<number, string> = {
  400: "Normal Draft",
  420: "Ranked Solo",
  430: "Blind Pick",
  440: "Ranked Flex",
  490: "Quickplay",
};

export function queueLabel(q: string): string {
  return OPGG_QUEUES[q] ?? q.charAt(0) + q.slice(1).toLowerCase();
}

export function clientQueueLabel(id: number | null): string {
  return (id !== null && CLIENT_QUEUES[id]) || "Summoner's Rift";
}

/** 1011 -> "16:51" */
export function duration(s: number): string {
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
}

export function timeAgo(unix: number, now: number = Date.now() / 1000): string {
  const d = Math.max(0, now - unix);
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.floor(d / 60)} min ago`;
  if (d < 86400) return `${Math.floor(d / 3600)} h ago`;
  const days = Math.floor(d / 86400);
  if (days < 7) return days === 1 ? "yesterday" : `${days} days ago`;
  return new Date(unix * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** (K + A) / D, "Perfect" without deaths. */
export function kdaRatio(k: number, d: number, a: number): string {
  return d === 0 ? (k + a > 0 ? "Perfect" : "0.00") : ((k + a) / d).toFixed(2);
}

export function perMin(x: number, seconds: number): number {
  return seconds > 0 ? x / (seconds / 60) : 0;
}

/** 13660 -> "13.7k", 950 -> "950" */
export function compact(n: number): string {
  if (n >= 100000) return `${Math.round(n / 1000)}k`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(Math.round(n));
}

export function teamOf(g: GameRecord, p: PlayerRecord) {
  return g.teams.find((t) => t.side === p.side) ?? null;
}

export function teamPlayers(g: GameRecord, side: string): PlayerRecord[] {
  return g.players.filter((p) => p.side === side);
}

/** Kill participation, 0-1, or null if the team got no kills. */
export function killParticipation(g: GameRecord, p: PlayerRecord): number | null {
  const kills = teamOf(g, p)?.kills || teamPlayers(g, p.side).reduce((s, x) => s + x.kills, 0);
  return kills > 0 ? Math.min(1, (p.kills + p.assists) / kills) : null;
}

/** Share of the team's damage to champions, 0-1. */
export function damageShare(g: GameRecord, p: PlayerRecord): number | null {
  const total = teamPlayers(g, p.side).reduce((s, x) => s + x.damage, 0);
  return total > 0 ? p.damage / total : null;
}

/** Same position on the other team. */
export function laneOpponent(g: GameRecord, p: PlayerRecord): PlayerRecord | null {
  if (!p.position) return null;
  return g.players.find((x) => x.side !== p.side && x.position === p.position) ?? null;
}

export function splitRiotId(id: string): [string, string] {
  const i = id.lastIndexOf("#");
  return i < 0 ? [id, ""] : [id.slice(0, i), id.slice(i)];
}
