//! Recent match history via OP.GG's `lol_list_summoner_matches`, used only for
//! premade detection. The tool returns the target summoner alone in each
//! game's `participants` (no teammates), so two players are linked by the same
//! match `id` appearing in both histories with the same `team_key`.

use std::time::Duration;

use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Same per-request timeout as profile lookups.
pub const TIMEOUT: Duration = super::profile::TIMEOUT;
/// Games per player (the tool allows 5-20).
pub const LIMIT: u32 = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchRecord {
    pub id: String,
    /// Unix seconds; None if OP.GG's timestamp couldn't be read.
    pub created_at: Option<i64>,
    pub game_type: String,
    /// The target's side in that game ("BLUE" / "RED", or an Arena team name).
    pub team: String,
}

pub fn matches_args(game_name: &str, tag_line: &str, region: &str) -> Value {
    json!({
        "game_name": game_name,
        "tag_line": tag_line,
        "region": region,
        "limit": LIMIT,
        "desired_output_fields": [
            "data.game_history[].{id,created_at,game_type}",
            "data.game_history[].participants[].{team_key}",
            "data.game_history[].participants[].summoner.{game_name,tagline}",
        ],
    })
}

fn same_id(p: &Value, game_name: &str, tag_line: &str) -> bool {
    let s = &p["summoner"];
    s["game_name"]
        .as_str()
        .is_some_and(|n| n.eq_ignore_ascii_case(game_name))
        && s["tagline"]
            .as_str()
            .is_some_and(|t| t.eq_ignore_ascii_case(tag_line))
}

/// Map the parsed tool output to one record per game. Games where the target's
/// side can't be determined are dropped.
pub fn parse_matches(
    v: &Value,
    game_name: &str,
    tag_line: &str,
) -> Result<Vec<MatchRecord>, String> {
    let games = v["data"]["game_history"]
        .as_array()
        .ok_or("no game_history in OP.GG response")?;
    Ok(games
        .iter()
        .filter_map(|g| {
            let parts = g["participants"].as_array()?;
            // Normally only the target is listed; if OP.GG ever returns the
            // whole lobby, pick the target by Riot ID.
            let me = match parts.as_slice() {
                [only] => only,
                _ => parts.iter().find(|p| same_id(p, game_name, tag_line))?,
            };
            let team = me["team_key"].as_str().filter(|t| !t.is_empty())?;
            Some(MatchRecord {
                id: g["id"].as_str().filter(|i| !i.is_empty())?.to_string(),
                created_at: g["created_at"]
                    .as_str()
                    .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
                    .map(|t| t.unix_timestamp()),
                game_type: g["game_type"].as_str().unwrap_or("").to_string(),
                team: team.to_string(),
            })
        })
        .collect())
}

pub async fn fetch(
    http: &reqwest::Client,
    game_name: &str,
    tag_line: &str,
    region: &str,
) -> Result<Vec<MatchRecord>, String> {
    let tree = super::call_tool(
        http,
        "lol_list_summoner_matches",
        matches_args(game_name, tag_line, region),
        TIMEOUT,
    )
    .await?;
    parse_matches(&tree, game_name, tag_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from a real call (Hide on bush#KR1, KR, limit 15) on 2026-10-17.
    fn fixture() -> Value {
        let rpc =
            super::super::parse_rpc_body(include_str!("../../fixtures/opgg-matches.rpc.json"), 3)
                .unwrap();
        super::super::textfmt::parse(&super::super::extract_text(&rpc).unwrap()).unwrap()
    }

    #[test]
    fn parses_real_match_list() {
        let m = parse_matches(&fixture(), "Hide on bush", "KR1").unwrap();
        assert_eq!(m.len(), 15);
        assert_eq!(
            m[0],
            MatchRecord {
                id: "m_eaDVNbCj-PJl0HG2MskNDjLMeO0tzpeHjxSn4RXwE=".into(),
                // 2026-10-17T01:53:14+09:00
                created_at: Some(1_792_169_594),
                game_type: "SOLORANKED".into(),
                team: "RED".into(),
            }
        );
        assert_eq!(m[1].team, "BLUE");
        // Arena games keep their team name; they only ever match the same team.
        assert!(
            m.iter()
                .any(|g| g.game_type == "ARENA" && g.team == "RAPTOR")
        );
        // Newest first.
        assert!(m.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    }

    #[test]
    fn picks_target_from_full_lobby_and_skips_bad_games() {
        let v = json!({"data":{"game_history":[
            {"id":"a","created_at":"2026-10-16T10:00:00Z","game_type":"NORMAL","participants":[
                {"team_key":"BLUE","summoner":{"game_name":"Other","tagline":"X"}},
                {"team_key":"RED","summoner":{"game_name":"me","tagline":"euw"}}]},
            {"id":"b","created_at":"bogus","game_type":"NORMAL","participants":[{"team_key":"BLUE"}]},
            {"id":"c","participants":[{"team_key":""}]},
            {"id":"","participants":[{"team_key":"RED"}]},
            {"id":"d","participants":[]}
        ]}});
        let m = parse_matches(&v, "Me", "EUW").unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id.as_str(), m[0].team.as_str()), ("a", "RED"));
        assert_eq!(m[0].created_at, Some(1_792_144_800));
        assert_eq!((m[1].id.as_str(), m[1].created_at), ("b", None));
        assert!(parse_matches(&json!({"data":{}}), "a", "b").is_err());
    }
}
