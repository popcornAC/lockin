//! Player lookup (rank / win rate / top champions) through OP.GG.
//!
//! Only players whose Riot ID the client exposes are looked up: in champ select
//! that means yourself and visible (e.g. party) teammates, never anonymised
//! ranked allies. In game, the Live Client Data API lists all ten players.
//! Profiles are cached for the app session; match lists (premade detection)
//! for [`MATCHES_TTL`], so the next game sees newer games.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::opgg::matches::{self, MatchRecord};
use crate::opgg::profile::{self, Profile};
use crate::premades::PartyTag;

/// Shorter than a game, so a rematch against the same players refetches.
pub const MATCHES_TTL: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlayerView {
    pub riot_id: String,
    pub champion: Option<String>,
    /// "ORDER" / "CHAOS" in game, None in champ select.
    pub team: Option<String>,
    pub is_self: bool,
    pub profile: Option<Profile>,
    pub error: Option<String>,
    pub loading: bool,
    /// Party marker: your own party (known) or an inferred one (in game only).
    pub party: Option<PartyTag>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivePlayer {
    pub game_name: String,
    pub tag_line: String,
    pub champion: String,
    pub team: String,
}

/// Parse `/liveclientdata/playerlist`. Bots and entries without a Riot ID are skipped.
pub fn parse_live_players(v: &Value) -> Vec<LivePlayer> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["isBot"].as_bool() != Some(true))
        .filter_map(|p| {
            let (name, tag) = match (p["riotIdGameName"].as_str(), p["riotIdTagLine"].as_str()) {
                (Some(n), Some(t)) if !n.is_empty() && !t.is_empty() => {
                    (n.to_string(), t.to_string())
                }
                _ => {
                    let (n, t) = p["riotId"].as_str()?.split_once('#')?;
                    (n.to_string(), t.to_string())
                }
            };
            Some(LivePlayer {
                game_name: name,
                tag_line: tag,
                champion: p["championName"].as_str().unwrap_or("").to_string(),
                team: p["team"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

pub fn riot_id_string(name: &str, tag: &str) -> String {
    format!("{name}#{tag}")
}

type MatchesEntry = (Instant, Result<Vec<MatchRecord>, String>);

#[derive(Default)]
pub struct Lookup {
    cache: Mutex<HashMap<String, Result<Profile, String>>>,
    matches: Mutex<HashMap<String, MatchesEntry>>,
}

/// Cache successes and "not found"; retry transient errors next time.
fn cacheable<T>(r: &Result<T, String>) -> bool {
    r.is_ok() || r.as_ref().is_err_and(|e| e.contains("not found"))
}

impl Lookup {
    pub fn cached(&self, region: &str, riot_id: &str) -> Option<Result<Profile, String>> {
        self.cache
            .lock()
            .unwrap()
            .get(&key(region, riot_id))
            .cloned()
    }

    /// Look up several players concurrently (each request has its own timeout).
    pub async fn lookup_many(
        &self,
        http: &reqwest::Client,
        region: &str,
        ids: &[(String, String)],
    ) -> Vec<Result<Profile, String>> {
        let futs = ids.iter().map(|(n, t)| async move {
            let rid = riot_id_string(n, t);
            if let Some(r) = self.cached(region, &rid) {
                return r;
            }
            let r = profile::fetch(http, n, t, region).await;
            if cacheable(&r) {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(key(region, &rid), r.clone());
            }
            r
        });
        futures_util::future::join_all(futs).await
    }

    /// Recent match lists for several players, concurrently.
    pub async fn matches_many(
        &self,
        http: &reqwest::Client,
        region: &str,
        ids: &[(String, String)],
    ) -> Vec<Result<Vec<MatchRecord>, String>> {
        let futs = ids.iter().map(|(n, t)| async move {
            let k = key(region, &riot_id_string(n, t));
            if let Some((at, r)) = self.matches.lock().unwrap().get(&k)
                && at.elapsed() < MATCHES_TTL
            {
                return r.clone();
            }
            let r = matches::fetch(http, n, t, region).await;
            if cacheable(&r) {
                self.matches
                    .lock()
                    .unwrap()
                    .insert(k, (Instant::now(), r.clone()));
            }
            r
        });
        futures_util::future::join_all(futs).await
    }
}

fn key(region: &str, riot_id: &str) -> String {
    format!("{region}|{}", riot_id.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_players() {
        let v = json!([
            {"riotId":"Alpha#EUW","riotIdGameName":"Alpha","riotIdTagLine":"EUW","championName":"Ahri","team":"ORDER","isBot":false},
            {"riotId":"Beta Name#1234","championName":"Garen","team":"CHAOS"},
            {"summonerName":"Bot","championName":"Annie","team":"CHAOS","isBot":true},
            {"summonerName":"NoId","championName":"Lux","team":"CHAOS"}
        ]);
        let p = parse_live_players(&v);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].game_name, "Alpha");
        assert_eq!(
            p[1],
            LivePlayer {
                game_name: "Beta Name".into(),
                tag_line: "1234".into(),
                champion: "Garen".into(),
                team: "CHAOS".into()
            }
        );
        assert!(parse_live_players(&json!({"error":"x"})).is_empty());
    }
}
