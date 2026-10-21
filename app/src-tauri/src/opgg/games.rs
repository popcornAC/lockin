//! Finished games via OP.GG: `lol_list_summoner_matches` to find them and
//! `lol_get_summoner_game_detail` for the full 10-player scoreboard.
//!
//! OP.GG game ids are opaque (encrypted), not the client's numeric gameId, so
//! a game that just ended is matched by its end time and champion instead.
//! OP.GG's `created_at` is when the game *ended* (checked against consecutive
//! games: it's never earlier than the previous game's time plus this game's
//! length, while "start time" would often overlap).
//! OP.GG reports no LP change, no vision score and only the keystone and the
//! two rune trees, so those are absent from [`GameRecord`].

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Game detail is a bigger response than a profile.
pub const TIMEOUT: Duration = Duration::from_secs(12);
/// Recent games to scan (the tool allows 5-20).
pub const LIST_LIMIT: u32 = 20;
/// Bump when [`GameRecord`] changes incompatibly; older records are refetched.
pub const RECORD_VERSION: u32 = 1;

/// Summoner's Rift queues we keep (OP.GG `game_type`). Skips ARAM, URF,
/// Arena, bots, customs and other rotating modes.
const SR_TYPES: [&str; 5] = ["SOLORANKED", "FLEXRANKED", "NORMAL", "BLIND", "QUICKPLAY"];

pub fn is_supported(game_map: &str, game_type: &str) -> bool {
    game_map == "SUMMONERS_RIFT" && SR_TYPES.contains(&game_type)
}

/// One row of the match list: enough to decide whether to fetch the detail.
#[derive(Debug, Clone, PartialEq)]
pub struct GameRef {
    pub id: String,
    /// OP.GG's own timestamp string; the detail call needs it verbatim.
    pub created_at: String,
    pub created_unix: i64,
    pub game_map: String,
    pub game_type: String,
    pub duration: u32,
    /// The target summoner's champion, if OP.GG lists it.
    pub champion_id: Option<i64>,
}

impl GameRef {
    pub fn supported(&self) -> bool {
        is_supported(&self.game_map, &self.game_type)
    }
    /// `created_at` is the end of the game (see the module docs).
    pub fn ended_unix(&self) -> i64 {
        self.created_unix
    }
}

pub fn list_args(game_name: &str, tag_line: &str, region: &str) -> Value {
    json!({
        "game_name": game_name,
        "tag_line": tag_line,
        "region": region,
        "limit": LIST_LIMIT,
        "desired_output_fields": [
            "data.game_history[].{created_at,game_length_second,game_map,game_type,id}",
            "data.game_history[].participants[].{champion_id}",
            "data.game_history[].participants[].summoner.{game_name,tagline}",
        ],
    })
}

fn unix(ts: &str) -> Option<i64> {
    OffsetDateTime::parse(ts, &Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
}

fn same_riot_id(s: &Value, game_name: &str, tag_line: &str) -> bool {
    s["game_name"]
        .as_str()
        .is_some_and(|n| n.eq_ignore_ascii_case(game_name))
        && s["tagline"]
            .as_str()
            .is_some_and(|t| t.eq_ignore_ascii_case(tag_line))
}

/// Parse the match list. Games without an id or a readable timestamp are dropped.
pub fn parse_list(v: &Value, game_name: &str, tag_line: &str) -> Result<Vec<GameRef>, String> {
    let games = v["data"]["game_history"]
        .as_array()
        .ok_or("no game_history in OP.GG response")?;
    Ok(games
        .iter()
        .filter_map(|g| {
            let created_at = g["created_at"].as_str()?.to_string();
            let parts = g["participants"].as_array();
            let me = parts.and_then(|p| match p.as_slice() {
                [only] => Some(only),
                many => many
                    .iter()
                    .find(|x| same_riot_id(&x["summoner"], game_name, tag_line)),
            });
            Some(GameRef {
                id: g["id"].as_str().filter(|i| !i.is_empty())?.to_string(),
                created_unix: unix(&created_at)?,
                created_at,
                game_map: g["game_map"].as_str().unwrap_or("").to_string(),
                game_type: g["game_type"].as_str().unwrap_or("").to_string(),
                duration: g["game_length_second"].as_u64().unwrap_or(0) as u32,
                champion_id: me.and_then(|m| m["champion_id"].as_i64()),
            })
        })
        .collect())
}

pub fn detail_args(region: &str, game: &GameRef, focus_riot_id: &str) -> Value {
    json!({
        "region": region,
        "game_id": game.id,
        "created_at": game.created_at,
        "focus_riot_id": focus_riot_id,
        "desired_output_fields": [
            "data.game_detail.{created_at,game_length_second,game_map,game_type,id}",
            "data.game_detail.average_tier_info.{division,tier}",
            "data.game_detail.teams[].{key,banned_champions[]}",
            "data.game_detail.teams[].game_stat.{baron_kill,champion_kill,dragon_kill,gold_earned,is_win,tower_kill}",
            "data.game_detail.teams[].participants[].{champion_id,champion_name,is_target,items[],position,role_bound_item,spells[],team_key}",
            "data.game_detail.teams[].participants[].rune.{primary_page_id,primary_rune_id,secondary_page_id}",
            "data.game_detail.teams[].participants[].stats.{assist,champion_level,death,gold_earned,kill,minion_kill,neutral_minion_kill,op_score,op_score_rank,result,total_damage_dealt_to_champions,total_damage_taken,vision_wards_bought_in_game,ward_place,largest_multi_kill}",
            "data.game_detail.teams[].participants[].summoner.{game_name,tagline}",
        ],
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamRecord {
    /// "BLUE" | "RED"
    pub side: String,
    pub win: bool,
    pub kills: u32,
    pub dragons: u32,
    pub barons: u32,
    pub towers: u32,
    pub gold: u32,
    pub bans: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub riot_id: String,
    pub side: String,
    /// LCU spelling: top, jungle, middle, bottom, utility (or "").
    pub position: String,
    pub champion_id: i64,
    pub champion: String,
    pub level: u32,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// Lane minions + jungle monsters.
    pub cs: u32,
    pub gold: u32,
    pub damage: u32,
    pub damage_taken: u32,
    pub wards_placed: u32,
    pub control_wards: u32,
    pub largest_multi_kill: u32,
    pub items: Vec<i64>,
    /// Role quest reward / 7th-slot item, if any.
    pub role_item: Option<i64>,
    pub spells: Vec<i64>,
    pub primary_style: Option<i64>,
    pub keystone: Option<i64>,
    pub sub_style: Option<i64>,
    /// OP.GG's 0-10 performance score and its rank in the game (1 = best).
    pub op_score: Option<f64>,
    pub op_rank: Option<u32>,
}

/// One stored game: the whole scoreboard, a few KB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub version: u32,
    pub id: String,
    pub region: String,
    /// Unix seconds when the game ended.
    pub ended_at: i64,
    pub duration: u32,
    /// OP.GG game type, e.g. "SOLORANKED".
    pub queue: String,
    /// Average rank of the lobby as OP.GG reports it, e.g. "GRANDMASTER 1".
    pub average_tier: Option<String>,
    pub teams: Vec<TeamRecord>,
    pub players: Vec<PlayerRecord>,
    /// Index of your own row in `players`.
    pub me: Option<usize>,
}

/// The list row shown for a stored game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSummary {
    pub id: String,
    pub ended_at: i64,
    pub duration: u32,
    pub queue: String,
    pub win: Option<bool>,
    pub champion_id: i64,
    pub champion: String,
    pub position: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
}

impl GameRecord {
    pub fn summary(&self) -> GameSummary {
        let me = self.me.and_then(|i| self.players.get(i));
        let win = me.and_then(|p| self.teams.iter().find(|t| t.side == p.side).map(|t| t.win));
        GameSummary {
            id: self.id.clone(),
            ended_at: self.ended_at,
            duration: self.duration,
            queue: self.queue.clone(),
            win,
            champion_id: me.map_or(0, |p| p.champion_id),
            champion: me.map(|p| p.champion.clone()).unwrap_or_default(),
            position: me.map(|p| p.position.clone()).unwrap_or_default(),
            kills: me.map_or(0, |p| p.kills),
            deaths: me.map_or(0, |p| p.deaths),
            assists: me.map_or(0, |p| p.assists),
            cs: me.map_or(0, |p| p.cs),
        }
    }
}

/// OP.GG position names to the client's.
pub fn lcu_position(p: &str) -> &'static str {
    match p.to_ascii_uppercase().as_str() {
        "TOP" => "top",
        "JUNGLE" => "jungle",
        "MID" | "MIDDLE" => "middle",
        "ADC" | "BOTTOM" | "BOT" => "bottom",
        "SUPPORT" | "UTILITY" => "utility",
        _ => "",
    }
}

fn n(v: &Value) -> u32 {
    v.as_u64().map_or(0, |x| x.min(u64::from(u32::MAX)) as u32)
}

fn ids(v: &Value) -> Vec<i64> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter(|&i| i > 0)
        .collect()
}

fn pos_id(v: &Value) -> Option<i64> {
    v.as_i64().filter(|&i| i > 0)
}

fn player(p: &Value, team_side: &str) -> Option<PlayerRecord> {
    let s = &p["stats"];
    let sm = &p["summoner"];
    let riot_id = match (sm["game_name"].as_str(), sm["tagline"].as_str()) {
        (Some(nm), Some(t)) if !nm.is_empty() => format!("{nm}#{t}"),
        (Some(nm), _) if !nm.is_empty() => nm.to_string(),
        _ => String::new(),
    };
    Some(PlayerRecord {
        riot_id,
        side: p["team_key"]
            .as_str()
            .filter(|t| !t.is_empty())
            .unwrap_or(team_side)
            .to_string(),
        position: lcu_position(p["position"].as_str().unwrap_or("")).to_string(),
        champion_id: p["champion_id"].as_i64()?,
        champion: p["champion_name"].as_str().unwrap_or("").to_string(),
        level: n(&s["champion_level"]),
        kills: n(&s["kill"]),
        deaths: n(&s["death"]),
        assists: n(&s["assist"]),
        cs: n(&s["minion_kill"]) + n(&s["neutral_minion_kill"]),
        gold: n(&s["gold_earned"]),
        damage: n(&s["total_damage_dealt_to_champions"]),
        damage_taken: n(&s["total_damage_taken"]),
        wards_placed: n(&s["ward_place"]),
        control_wards: n(&s["vision_wards_bought_in_game"]),
        largest_multi_kill: n(&s["largest_multi_kill"]),
        items: ids(&p["items"]),
        role_item: pos_id(&p["role_bound_item"]),
        spells: ids(&p["spells"]),
        primary_style: pos_id(&p["rune"]["primary_page_id"]),
        keystone: pos_id(&p["rune"]["primary_rune_id"]),
        sub_style: pos_id(&p["rune"]["secondary_page_id"]),
        op_score: s["op_score"].as_f64(),
        op_rank: s["op_score_rank"].as_u64().map(|r| r as u32),
    })
}

const POS_ORDER: [&str; 5] = ["top", "jungle", "middle", "bottom", "utility"];

/// Parse a game detail. `me` is your Riot ID, used when OP.GG didn't flag a
/// target row. Players are sorted blue then red, each top to support.
pub fn parse_detail(v: &Value, region: &str, me: &str) -> Result<GameRecord, String> {
    let d = &v["data"]["game_detail"];
    let id = d["id"]
        .as_str()
        .filter(|i| !i.is_empty())
        .ok_or("no game in OP.GG response")?;
    let ended_at = d["created_at"]
        .as_str()
        .and_then(unix)
        .ok_or("game has no readable time")?;
    let teams_v = d["teams"].as_array().ok_or("game has no teams")?;
    let mut teams = Vec::new();
    let mut players: Vec<(bool, PlayerRecord)> = Vec::new();
    for t in teams_v {
        let side = t["key"].as_str().unwrap_or("").to_string();
        let gs = &t["game_stat"];
        teams.push(TeamRecord {
            side: side.clone(),
            win: gs["is_win"].as_bool().unwrap_or(false),
            kills: n(&gs["champion_kill"]),
            dragons: n(&gs["dragon_kill"]),
            barons: n(&gs["baron_kill"]),
            towers: n(&gs["tower_kill"]),
            gold: n(&gs["gold_earned"]),
            bans: ids(&t["banned_champions"]),
        });
        for p in t["participants"].as_array().into_iter().flatten() {
            if let Some(rec) = player(p, &side) {
                players.push((p["is_target"].as_bool() == Some(true), rec));
            }
        }
    }
    if players.is_empty() {
        return Err("game has no players".into());
    }
    let side_rank = |s: &str| if s == "BLUE" { 0 } else { 1 };
    let pos_rank = |p: &str| POS_ORDER.iter().position(|x| *x == p).unwrap_or(5);
    players.sort_by_key(|(_, p)| (side_rank(&p.side), pos_rank(&p.position)));
    teams.sort_by_key(|t| side_rank(&t.side));
    let me_idx = players.iter().position(|(t, _)| *t).or_else(|| {
        players
            .iter()
            .position(|(_, p)| !me.is_empty() && p.riot_id.eq_ignore_ascii_case(me))
    });
    let average_tier = d["average_tier_info"]["tier"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(|t| match d["average_tier_info"]["division"].as_i64() {
            Some(div) if div > 0 => format!("{t} {div}"),
            _ => t.to_string(),
        });
    Ok(GameRecord {
        version: RECORD_VERSION,
        id: id.to_string(),
        region: region.to_string(),
        ended_at,
        duration: n(&d["game_length_second"]),
        queue: d["game_type"].as_str().unwrap_or("").to_string(),
        average_tier,
        teams,
        players: players.into_iter().map(|(_, p)| p).collect(),
        me: me_idx,
    })
}

pub async fn fetch_list(
    http: &reqwest::Client,
    game_name: &str,
    tag_line: &str,
    region: &str,
) -> Result<Vec<GameRef>, String> {
    let tree = super::call_tool(
        http,
        "lol_list_summoner_matches",
        list_args(game_name, tag_line, region),
        TIMEOUT,
    )
    .await?;
    parse_list(&tree, game_name, tag_line)
}

pub async fn fetch_detail(
    http: &reqwest::Client,
    region: &str,
    game: &GameRef,
    riot_id: &str,
) -> Result<GameRecord, String> {
    let tree = super::call_tool(
        http,
        "lol_get_summoner_game_detail",
        detail_args(region, game, riot_id),
        TIMEOUT,
    )
    .await?;
    parse_detail(&tree, region, riot_id)
}

#[cfg(test)]
mod tests {
    use super::super::textfmt;
    use super::*;

    // Captured 2026-10-21 (Hide on bush#KR1, KR); other players' Riot IDs anonymised.
    fn win() -> Value {
        textfmt::parse(include_str!("../../fixtures/opgg-game-detail-win.txt")).unwrap()
    }
    fn loss() -> Value {
        textfmt::parse(include_str!("../../fixtures/opgg-game-detail-loss.txt")).unwrap()
    }

    #[test]
    fn parses_game_list() {
        let v = textfmt::parse(include_str!("../../fixtures/opgg-game-list.txt")).unwrap();
        let g = parse_list(&v, "hide on bush", "kr1").unwrap();
        assert_eq!(g.len(), 20);
        assert_eq!(
            g[0],
            GameRef {
                id: "m_eaDVNbCj-PJl0HG2MskNDjLMeO0tzpeHjxSn4RXwE=".into(),
                created_at: "2026-10-17T01:53:14+09:00".into(),
                created_unix: 1_792_169_594,
                game_map: "SUMMONERS_RIFT".into(),
                game_type: "SOLORANKED".into(),
                duration: 1011,
                champion_id: Some(3),
            }
        );
        assert_eq!(g[0].ended_unix(), 1_792_169_594);
        assert!(g[0].supported());
        let arena: Vec<_> = g.iter().filter(|x| !x.supported()).collect();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena[0].game_type, "ARENA");
        assert!(parse_list(&json!({"data":{}}), "a", "b").is_err());
    }

    #[test]
    fn parses_winning_game() {
        let r = parse_detail(&win(), "KR", "Hide on bush#KR1").unwrap();
        assert_eq!(r.id, "m_eaDVNbCj-PJl0HG2MskNDjLMeO0tzpeHjxSn4RXwE=");
        assert_eq!(r.ended_at, 1_792_169_594);
        assert_eq!(r.duration, 1011);
        assert_eq!(r.queue, "SOLORANKED");
        assert_eq!(r.average_tier.as_deref(), Some("GRANDMASTER 1"));
        assert_eq!(r.players.len(), 10);
        assert_eq!(r.teams.len(), 2);
        assert_eq!(
            r.teams[0],
            TeamRecord {
                side: "BLUE".into(),
                win: false,
                kills: 14,
                dragons: 1,
                barons: 0,
                towers: 0,
                gold: 34679,
                bans: vec![76, 111, 157],
            }
        );
        assert!(r.teams[1].win);
        // Blue first, then red; each team top to support.
        let order: Vec<_> = r
            .players
            .iter()
            .map(|p| (p.side.as_str(), p.position.as_str()))
            .collect();
        assert_eq!(order[0], ("BLUE", "top"));
        assert_eq!(order[4], ("BLUE", "utility"));
        assert_eq!(order[5], ("RED", "top"));
        assert_eq!(order[9], ("RED", "utility"));
        let me = &r.players[r.me.unwrap()];
        assert_eq!(
            *me,
            PlayerRecord {
                riot_id: "Hide on bush#KR1".into(),
                side: "RED".into(),
                position: "middle".into(),
                champion_id: 3,
                champion: "Galio".into(),
                level: 12,
                kills: 7,
                deaths: 4,
                assists: 6,
                cs: 123,
                gold: 8205,
                damage: 13660,
                damage_taken: 15000,
                wards_placed: 5,
                control_wards: 3,
                largest_multi_kill: 2,
                items: vec![3152, 3173, 2055, 4005, 1056, 1082],
                role_item: Some(1206),
                spells: vec![12, 4],
                primary_style: Some(8200),
                keystone: Some(8230),
                sub_style: Some(8400),
                op_score: Some(6.93),
                op_rank: Some(4),
            }
        );
        let s = r.summary();
        assert_eq!(s.win, Some(true));
        assert_eq!((s.kills, s.deaths, s.assists, s.cs), (7, 4, 6, 123));
        assert_eq!(
            (s.champion.as_str(), s.position.as_str()),
            ("Galio", "middle")
        );
        // Every row has an OP Score and a 1-10 rank.
        let mut ranks: Vec<_> = r.players.iter().map(|p| p.op_rank.unwrap()).collect();
        ranks.sort();
        assert_eq!(ranks, (1..=10).collect::<Vec<_>>());
    }

    #[test]
    fn parses_losing_game_with_integer_scores() {
        let r = parse_detail(&loss(), "KR", "").unwrap();
        let me = &r.players[r.me.unwrap()];
        assert_eq!(me.champion, "LeBlanc");
        assert_eq!(r.summary().win, Some(false));
        // OP Score 10 / 4 is printed without a decimal point.
        assert!(r.players.iter().any(|p| p.op_score == Some(10.0)));
        // Fewer than 6 items is fine.
        assert!(r.players.iter().any(|p| p.items.len() == 3));
        assert_eq!(r.teams[1].bans.len(), 5);
    }

    #[test]
    fn falls_back_to_riot_id_and_tolerates_gaps() {
        let mut v = win();
        for t in v["data"]["game_detail"]["teams"].as_array_mut().unwrap() {
            for p in t["participants"].as_array_mut().unwrap() {
                p["is_target"] = json!(false);
                p["stats"]["op_score"] = Value::Null;
            }
        }
        let r = parse_detail(&v, "KR", "HIDE ON BUSH#kr1").unwrap();
        assert_eq!(r.players[r.me.unwrap()].champion, "Galio");
        assert!(r.players.iter().all(|p| p.op_score.is_none()));
        // Unknown Riot ID: no "me", summary has no result.
        let r = parse_detail(&v, "KR", "Someone#Else").unwrap();
        assert_eq!(r.me, None);
        assert_eq!(r.summary().win, None);
        assert!(parse_detail(&json!({}), "KR", "").is_err());
        assert!(parse_detail(&json!({"data":{"game_detail":{"id":"x","created_at":"2026-10-17T01:53:14+09:00","teams":[]}}}), "KR", "").is_err());
    }

    #[test]
    fn queue_filter_and_positions() {
        assert!(is_supported("SUMMONERS_RIFT", "FLEXRANKED"));
        assert!(!is_supported("SUMMONERS_RIFT", "URF"));
        assert!(!is_supported("HOWLING_ABYSS", "ARAM"));
        assert!(!is_supported("ARENA_MAP_1", "ARENA"));
        assert_eq!(lcu_position("ADC"), "bottom");
        assert_eq!(lcu_position("SUPPORT"), "utility");
        assert_eq!(lcu_position(""), "");
    }
}
