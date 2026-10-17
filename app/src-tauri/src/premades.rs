//! Party ("duo / trio / 5-stack") markers.
//!
//! - **Your own party** comes from the League client's lobby, so it is known.
//! - **Other parties** are only inferred in game, when the Live Client Data API
//!   already shows every Riot ID: two players on the same side are a likely
//!   premade if their recent OP.GG histories share at least
//!   [`MIN_GAMES_TOGETHER`] games (same match id, same side) in the last
//!   [`RECENT_DAYS`] days. Pairs are grouped into parties with union-find.
//!
//! Nothing here ever runs for champ select's anonymous "Ally #" players: in
//! champ select only the known party is marked.

use std::collections::HashMap;

use petgraph::unionfind::UnionFind;
use serde::Serialize;

use crate::opgg::matches::MatchRecord;

pub const MIN_GAMES_TOGETHER: u32 = 2;
pub const RECENT_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PartyTag {
    /// Stable per game: 0 is your own party, inferred parties count from 1.
    /// The UI uses it for the party colour.
    pub id: u32,
    pub size: u32,
    /// Inferred from match history rather than known from the client.
    pub likely: bool,
    /// Most recent games this player shared with a party member (inferred only).
    pub games: Option<u32>,
}

/// Same-side games two histories share within the recency window.
pub fn games_together(a: &[MatchRecord], b: &[MatchRecord], now: i64) -> u32 {
    let cutoff = now - RECENT_DAYS * 86_400;
    let sides: HashMap<&str, &str> = a
        .iter()
        // An unreadable timestamp is not evidence of recency.
        .filter(|m| m.created_at.is_some_and(|t| t >= cutoff))
        .map(|m| (m.id.as_str(), m.team.as_str()))
        .collect();
    b.iter()
        .filter(|m| sides.get(m.id.as_str()) == Some(&m.team.as_str()))
        .count() as u32
}

/// Likely premade pairs `(i, j, games)` with `i < j`. `teams[i]` is player i's
/// side in the current game; `histories[i]` is None when not fetched or failed.
pub fn likely_pairs(
    teams: &[&str],
    histories: &[Option<&[MatchRecord]>],
    now: i64,
) -> Vec<(usize, usize, u32)> {
    let mut out = Vec::new();
    for i in 0..teams.len() {
        for j in i + 1..teams.len() {
            if teams[i].is_empty() || teams[i] != teams[j] {
                continue; // premades are always on the same side
            }
            let (Some(Some(a)), Some(Some(b))) = (histories.get(i), histories.get(j)) else {
                continue;
            };
            let n = games_together(a, b, now);
            if n >= MIN_GAMES_TOGETHER {
                out.push((i, j, n));
            }
        }
    }
    out
}

/// Union-find over the pairs; returns groups of two or more, each sorted, in
/// order of their first member.
pub fn group(n: usize, pairs: &[(usize, usize, u32)]) -> Vec<Vec<usize>> {
    let mut uf = UnionFind::<usize>::new(n);
    for &(a, b, _) in pairs {
        uf.union(a, b);
    }
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        by_root.entry(uf.find(i)).or_default().push(i);
    }
    let mut groups: Vec<Vec<usize>> = by_root.into_values().filter(|g| g.len() > 1).collect();
    groups.sort_by_key(|g| g[0]);
    groups
}

/// Tags per player (indexed like the input). `known` is your own party's
/// indices; inferred pairs touching it are ignored, because the client already
/// says exactly who you queued with.
pub fn tags(n: usize, known: &[usize], pairs: &[(usize, usize, u32)]) -> Vec<Option<PartyTag>> {
    let mut out = vec![None; n];
    if known.len() > 1 {
        for &i in known.iter().filter(|&&i| i < n) {
            out[i] = Some(PartyTag {
                id: 0,
                size: known.len() as u32,
                likely: false,
                games: None,
            });
        }
    }
    let pairs: Vec<_> = pairs
        .iter()
        .copied()
        .filter(|(a, b, _)| *a < n && *b < n && !known.contains(a) && !known.contains(b))
        .collect();
    for (k, g) in group(n, &pairs).into_iter().enumerate() {
        for &i in &g {
            let games = pairs
                .iter()
                .filter(|(a, b, _)| *a == i || *b == i)
                .map(|p| p.2)
                .max();
            out[i] = Some(PartyTag {
                id: k as u32 + 1,
                size: g.len() as u32,
                likely: true,
                games,
            });
        }
    }
    out
}

/// Which players need a match-history request: not in your known party, and
/// on a side with at least two such players (a lone player can't pair).
pub fn needs_history(teams: &[&str], known: &[usize]) -> Vec<bool> {
    let cand = |i: usize| !teams[i].is_empty() && !known.contains(&i);
    (0..teams.len())
        .map(|i| cand(i) && (0..teams.len()).any(|j| j != i && cand(j) && teams[j] == teams[i]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_200_000;
    const DAY: i64 = 86_400;

    fn m(id: &str, team: &str, age_days: i64) -> MatchRecord {
        MatchRecord {
            id: id.into(),
            created_at: Some(NOW - age_days * DAY),
            game_type: "SOLORANKED".into(),
            team: team.into(),
        }
    }

    #[test]
    fn counts_same_side_recent_games_only() {
        let a = [
            m("g1", "BLUE", 0),
            m("g2", "RED", 1),
            m("g3", "BLUE", 2),
            m("old", "BLUE", 30),
        ];
        let b = [
            m("g1", "BLUE", 0),
            m("g2", "BLUE", 1),
            m("g3", "BLUE", 2),
            m("old", "BLUE", 30),
            m("x", "RED", 0),
        ];
        // g2: opposite sides; old: outside the window.
        assert_eq!(games_together(&a, &b, NOW), 2);
        assert_eq!(games_together(&a, &[], NOW), 0);
        let mut undated = m("g1", "BLUE", 0);
        undated.created_at = None;
        assert_eq!(games_together(&[undated], &b, NOW), 0);
    }

    #[test]
    fn pairs_need_two_games_and_same_current_side() {
        let duo = vec![m("a", "BLUE", 0), m("b", "RED", 1)];
        let once = vec![m("a", "BLUE", 0)];
        let h: Vec<Option<&[MatchRecord]>> =
            vec![Some(&duo), Some(&duo), Some(&once), Some(&duo), None];
        let teams = ["ORDER", "ORDER", "ORDER", "CHAOS", "ORDER"];
        // 0-1: 2 games, same side. 0-2 / 1-2: only 1. 3: other side. 4: no history.
        assert_eq!(likely_pairs(&teams, &h, NOW), vec![(0, 1, 2)]);
        assert!(likely_pairs(&["", ""], &[Some(&duo), Some(&duo)], NOW).is_empty());
    }

    #[test]
    fn union_find_merges_chains_into_parties() {
        // 0-1, 1-2 => trio even without a direct 0-2 edge; 5-7 duo; 3, 4, 6 solo.
        let g = group(8, &[(1, 2, 2), (0, 1, 3), (5, 7, 2)]);
        assert_eq!(g, vec![vec![0, 1, 2], vec![5, 7]]);
        assert!(group(3, &[]).is_empty());
        assert_eq!(
            group(5, &[(0, 1, 2), (1, 2, 2), (2, 3, 2), (3, 4, 2)]),
            vec![vec![0, 1, 2, 3, 4]]
        );
    }

    #[test]
    fn tags_known_party_first_and_ignore_inferred_edges_touching_it() {
        // Known: me (2) + 3. Inferred: 0-1 duo, 3-4 (touches known, ignored), 6-7-8 trio.
        let pairs = [(0, 1, 2), (3, 4, 5), (6, 7, 2), (7, 8, 4)];
        let t = tags(10, &[2, 3], &pairs);
        let known = PartyTag {
            id: 0,
            size: 2,
            likely: false,
            games: None,
        };
        assert_eq!(t[2], Some(known));
        assert_eq!(t[3], Some(known));
        assert_eq!(t[4], None);
        assert_eq!(
            t[0],
            Some(PartyTag {
                id: 1,
                size: 2,
                likely: true,
                games: Some(2)
            })
        );
        assert_eq!(
            t[7],
            Some(PartyTag {
                id: 2,
                size: 3,
                likely: true,
                games: Some(4)
            })
        );
        assert_eq!(t[6].unwrap().games, Some(2));
        assert_eq!(t[5], None);
        // Solo queue: you alone are not a party.
        assert!(tags(3, &[1], &[]).iter().all(Option::is_none));
    }

    #[test]
    fn only_pairable_players_cost_a_request() {
        // ORDER: you (0) + your duo (1) are known, so only 2-4 need histories; all of CHAOS does.
        let teams = [
            "ORDER", "ORDER", "ORDER", "ORDER", "ORDER", "CHAOS", "CHAOS", "CHAOS", "CHAOS",
            "CHAOS",
        ];
        let need = needs_history(&teams, &[0, 1]);
        assert_eq!(need.iter().filter(|x| **x).count(), 8);
        assert!(!need[0] && !need[1]);
        // A 4-stack leaves one lone teammate: nothing to compare them with.
        let need = needs_history(&teams, &[0, 1, 2, 3]);
        assert!(!need[4]);
        assert_eq!(need.iter().filter(|x| **x).count(), 5);
        assert!(!needs_history(&["ORDER", ""], &[]).iter().any(|x| *x));
    }
}
