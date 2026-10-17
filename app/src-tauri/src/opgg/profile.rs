//! Player profiles via OP.GG's `lol_get_summoner_profile` (rank, LP, win rate,
//! most-played champions). No MMR estimates, no derived "dodge" signals.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

pub const TIMEOUT: Duration = Duration::from_secs(6);

/// Region codes OP.GG accepts (from its validation message).
const REGIONS: [&str; 34] = [
    "KR", "BR", "EUNE", "EUW", "LAN", "LAS", "NA", "OCE", "RU", "TR", "JP", "PH", "SG", "TH", "TW",
    "VN", "ME", "SEA", "KR1", "JP1", "NA1", "EUW1", "EUN1", "OC1", "BR1", "LA2", "LA1", "TR1",
    "PH2", "SG2", "TH2", "TW2", "VN2", "ME1",
];

/// Normalise the client's region / platform id to something OP.GG accepts.
pub fn opgg_region(lcu_region: &str) -> Option<String> {
    let r = lcu_region.trim().to_ascii_uppercase();
    let r = match r.as_str() {
        "RU1" => "RU".to_string(),
        "OC" => "OCE".to_string(),
        "EUN" => "EUNE".to_string(),
        _ => r,
    };
    REGIONS.contains(&r.as_str()).then_some(r)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rank {
    pub tier: String,
    pub division: Option<i64>,
    pub lp: Option<i64>,
    pub wins: u64,
    pub losses: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChampStat {
    pub name: String,
    pub games: u64,
    pub wins: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    pub level: Option<i64>,
    pub solo: Option<Rank>,
    pub flex: Option<Rank>,
    pub top_champions: Vec<ChampStat>,
}

pub fn profile_args(game_name: &str, tag_line: &str, region: &str) -> Value {
    json!({
        "game_name": game_name,
        "tag_line": tag_line,
        "region": region,
        "desired_output_fields": [
            "data.summoner.{game_name,tagline,level}",
            "data.summoner.league_stats[].{game_type,win,lose}",
            "data.summoner.league_stats[].tier_info.{tier,division,lp}",
            "data.summoner.most_champions.champion_stats[].{champion_name,play,win}",
        ],
    })
}

fn rank(stat: &Value) -> Option<Rank> {
    let t = &stat["tier_info"];
    let tier = t["tier"].as_str().filter(|s| !s.is_empty())?;
    Some(Rank {
        tier: tier.to_string(),
        division: t["division"].as_i64(),
        lp: t["lp"].as_i64(),
        wins: stat["win"].as_u64().unwrap_or(0),
        losses: stat["lose"].as_u64().unwrap_or(0),
    })
}

pub fn parse_profile(v: &Value) -> Result<Profile, String> {
    let s = &v["data"]["summoner"];
    if !s.is_object() {
        return Err("no summoner in OP.GG response".into());
    }
    let stats = s["league_stats"].as_array().cloned().unwrap_or_default();
    let find = |t: &str| {
        stats
            .iter()
            .find(|x| x["game_type"].as_str() == Some(t))
            .and_then(rank)
    };
    let mut champs: Vec<ChampStat> = s["most_champions"]["champion_stats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(ChampStat {
                name: c["champion_name"].as_str()?.to_string(),
                games: c["play"].as_u64().unwrap_or(0),
                wins: c["win"].as_u64().unwrap_or(0),
            })
        })
        .collect();
    champs.truncate(3);
    Ok(Profile {
        level: s["level"].as_i64(),
        solo: find("SOLORANKED"),
        flex: find("FLEXRANKED"),
        top_champions: champs,
    })
}

pub async fn fetch(
    http: &reqwest::Client,
    game_name: &str,
    tag_line: &str,
    region: &str,
) -> Result<Profile, String> {
    let tree = super::call_tool(
        http,
        "lol_get_summoner_profile",
        profile_args(game_name, tag_line, region),
        TIMEOUT,
    )
    .await?;
    parse_profile(&tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_fixture() {
        let tree =
            super::super::textfmt::parse(include_str!("../../fixtures/opgg-profile.txt")).unwrap();
        let p = parse_profile(&tree).unwrap();
        assert_eq!(p.level, Some(946));
        assert_eq!(
            p.solo,
            Some(Rank {
                tier: "CHALLENGER".into(),
                division: Some(1),
                lp: Some(2256),
                wins: 448,
                losses: 371
            })
        );
        assert_eq!(p.flex, None); // unranked: tier null
        assert_eq!(p.top_champions.len(), 3);
        assert_eq!(
            p.top_champions[0],
            ChampStat {
                name: "Yone".into(),
                games: 63,
                wins: 31
            }
        );
    }

    #[test]
    fn bad_profile() {
        assert!(parse_profile(&json!({})).is_err());
    }

    #[test]
    fn regions() {
        assert_eq!(opgg_region("euw").as_deref(), Some("EUW"));
        assert_eq!(opgg_region("EUW1").as_deref(), Some("EUW1"));
        assert_eq!(opgg_region("LA1").as_deref(), Some("LA1"));
        assert_eq!(opgg_region("RU1").as_deref(), Some("RU"));
        assert_eq!(opgg_region("PBE"), None);
    }
}
