//! Typed wrappers around the LCU endpoints we use, plus pure parsers for their
//! payloads. Everything that knows LCU paths or JSON shapes lives here, so a
//! patch that changes the client only needs fixing in this module.

use serde_json::{Value, json};

use super::client::{LcuClient, LcuResult};

// ---------- gameflow / ready check ----------

pub async fn gameflow_phase(c: &LcuClient) -> LcuResult<String> {
    Ok(c.get("/lol-gameflow/v1/gameflow-phase")
        .await?
        .as_str()
        .unwrap_or("None")
        .to_string())
}

/// Map id and queue id of the current lobby/game, from the gameflow session.
pub async fn gameflow_map_queue(c: &LcuClient) -> LcuResult<(Option<i64>, Option<i64>)> {
    let s = c.get("/lol-gameflow/v1/session").await?;
    Ok(parse_map_queue(&s))
}

pub fn parse_map_queue(session: &Value) -> (Option<i64>, Option<i64>) {
    let queue = &session["gameData"]["queue"];
    let map = session["map"]["id"]
        .as_i64()
        .or_else(|| queue["mapId"].as_i64());
    (map, queue["id"].as_i64())
}

pub async fn ready_check(c: &LcuClient) -> LcuResult<Value> {
    c.get("/lol-matchmaking/v1/ready-check").await
}

pub async fn accept_ready_check(c: &LcuClient) -> LcuResult<Value> {
    c.post("/lol-matchmaking/v1/ready-check/accept", &Value::Null)
        .await
}

/// Accept only while the check is running and we haven't answered yet (so a
/// manual decline is never overridden).
pub fn should_accept(ready_check: &Value) -> bool {
    ready_check["state"].as_str() == Some("InProgress")
        && ready_check["playerResponse"].as_str() == Some("None")
}

// ---------- champ select ----------

pub async fn champ_select_session(c: &LcuClient) -> LcuResult<Value> {
    c.get("/lol-champ-select/v1/session").await
}

pub async fn set_summoner_spells(c: &LcuClient, spell1: i64, spell2: i64) -> LcuResult<Value> {
    c.patch(
        "/lol-champ-select/v1/session/my-selection",
        &json!({ "spell1Id": spell1, "spell2Id": spell2 }),
    )
    .await
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Ally {
    pub cell_id: i64,
    pub champion_id: i64,
    pub is_local: bool,
    /// Riot ID, only when the client exposes it (never for anonymised ranked allies).
    pub riot_id: Option<(String, String)>,
    /// PUUID usable for a lookup, only when the name is explicitly visible.
    pub visible_puuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Selection {
    pub game_id: i64,
    pub local_cell: i64,
    /// Champion locked in (or assigned, e.g. in modes without a pick action).
    pub locked_champion: i64,
    /// Champion currently hovered / intended (0 if none).
    pub hover_champion: i64,
    /// Lower-case LCU position: top, jungle, middle, bottom, utility, or "".
    pub position: String,
    pub spells: (i64, i64),
    pub allies: Vec<Ally>,
}

impl Selection {
    pub fn is_locked(&self) -> bool {
        self.locked_champion > 0
    }
    /// The champion to import for: locked if any, otherwise the hovered one.
    pub fn current_champion(&self) -> i64 {
        if self.locked_champion > 0 {
            self.locked_champion
        } else {
            self.hover_champion
        }
    }
}

fn ally_identity(m: &Value, is_local: bool) -> (Option<(String, String)>, Option<String>) {
    let vis = m["nameVisibilityType"].as_str().unwrap_or("");
    if vis.eq_ignore_ascii_case("HIDDEN") && !is_local {
        // Ranked "Ally #": never look these players up.
        return (None, None);
    }
    let name = m["gameName"].as_str().unwrap_or("").trim();
    let tag = m["tagLine"].as_str().unwrap_or("").trim();
    let riot_id =
        (!name.is_empty() && !tag.is_empty()).then(|| (name.to_string(), tag.to_string()));
    let explicitly_visible =
        is_local || vis.eq_ignore_ascii_case("VISIBLE") || vis.eq_ignore_ascii_case("UNHIDDEN");
    let puuid = m["puuid"]
        .as_str()
        .filter(|p| explicitly_visible && !p.is_empty())
        .map(String::from);
    (riot_id, puuid)
}

/// Parse `/lol-champ-select/v1/session` into what we need.
pub fn parse_selection(session: &Value) -> Option<Selection> {
    let local_cell = session["localPlayerCellId"].as_i64()?;
    let my_team = session["myTeam"].as_array()?;
    let me = my_team
        .iter()
        .find(|m| m["cellId"].as_i64() == Some(local_cell))?;

    let my_picks: Vec<&Value> = session["actions"]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .filter_map(|g| g.as_array())
                .flatten()
                .filter(|a| {
                    a["actorCellId"].as_i64() == Some(local_cell)
                        && a["type"].as_str() == Some("pick")
                })
                .collect()
        })
        .unwrap_or_default();

    let team_champ = me["championId"].as_i64().unwrap_or(0);
    let intent = me["championPickIntent"].as_i64().unwrap_or(0);
    let finalization = session["timer"]["phase"].as_str() == Some("FINALIZATION");

    let (locked, hover) = if my_picks.is_empty() {
        // No pick action (e.g. all-random modes): the assigned champion counts as locked.
        (team_champ, intent)
    } else {
        let locked = my_picks
            .iter()
            .find(|a| a["completed"].as_bool() == Some(true))
            .and_then(|a| a["championId"].as_i64())
            .filter(|c| *c > 0)
            .or_else(|| (finalization && team_champ > 0).then_some(team_champ))
            .unwrap_or(0);
        let hover = my_picks
            .iter()
            .find(|a| a["completed"].as_bool() != Some(true))
            .and_then(|a| a["championId"].as_i64())
            .filter(|c| *c > 0)
            .unwrap_or(if intent > 0 { intent } else { team_champ });
        (locked, hover)
    };

    let allies = my_team
        .iter()
        .map(|m| {
            let cell = m["cellId"].as_i64().unwrap_or(-1);
            let is_local = cell == local_cell;
            let (riot_id, visible_puuid) = ally_identity(m, is_local);
            Ally {
                cell_id: cell,
                champion_id: m["championId"].as_i64().unwrap_or(0),
                is_local,
                riot_id,
                visible_puuid,
            }
        })
        .collect();

    Some(Selection {
        game_id: session["gameId"].as_i64().unwrap_or(0),
        local_cell,
        locked_champion: locked,
        hover_champion: hover,
        position: me["assignedPosition"]
            .as_str()
            .unwrap_or("")
            .to_ascii_lowercase(),
        spells: (
            me["spell1Id"].as_i64().unwrap_or(0),
            me["spell2Id"].as_i64().unwrap_or(0),
        ),
        allies,
    })
}

// ---------- lobby (your own party) ----------

/// One member of your own lobby / party.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartyMember {
    pub puuid: String,
    /// Riot ID if the lobby payload carries it (resolve by PUUID otherwise).
    pub riot_id: Option<(String, String)>,
}

pub async fn lobby(c: &LcuClient) -> LcuResult<Value> {
    c.get("/lol-lobby/v2/lobby").await
}

/// `members` of `/lol-lobby/v2/lobby`. Empty when not in a lobby.
pub fn parse_lobby_members(lobby: &Value) -> Vec<PartyMember> {
    lobby["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let puuid = m["puuid"].as_str().filter(|p| !p.is_empty())?.to_string();
            let name = m["gameName"].as_str().unwrap_or("").trim();
            let tag = m["gameTag"]
                .as_str()
                .or(m["tagLine"].as_str())
                .unwrap_or("")
                .trim();
            let riot_id =
                (!name.is_empty() && !tag.is_empty()).then(|| (name.to_string(), tag.to_string()));
            Some(PartyMember { puuid, riot_id })
        })
        .collect()
}

/// Champ select cells that belong to your own party (matched by PUUID against
/// the lobby). Only ever marks people you queued with.
pub fn party_cells(session: &Value, party: &[PartyMember]) -> Vec<i64> {
    session["myTeam"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            m["puuid"]
                .as_str()
                .is_some_and(|p| !p.is_empty() && party.iter().any(|x| x.puuid == p))
        })
        .filter_map(|m| m["cellId"].as_i64())
        .collect()
}

// ---------- summoner / region ----------

pub struct Summoner {
    pub summoner_id: i64,
    pub account_id: i64,
    pub riot_id: Option<(String, String)>,
}

pub async fn current_summoner(c: &LcuClient) -> LcuResult<Summoner> {
    let v = c.get("/lol-summoner/v1/current-summoner").await?;
    let name = v["gameName"].as_str().unwrap_or("").to_string();
    let tag = v["tagLine"].as_str().unwrap_or("").to_string();
    Ok(Summoner {
        summoner_id: v["summonerId"].as_i64().unwrap_or(0),
        account_id: v["accountId"].as_i64().unwrap_or(0),
        riot_id: (!name.is_empty() && !tag.is_empty()).then_some((name, tag)),
    })
}

/// Riot ID for a PUUID, used only for players whose name the client marks visible.
pub async fn riot_id_by_puuid(c: &LcuClient, puuid: &str) -> LcuResult<Option<(String, String)>> {
    let v = c
        .get(&format!("/lol-summoner/v2/summoners/puuid/{puuid}"))
        .await?;
    let name = v["gameName"].as_str().unwrap_or("");
    let tag = v["tagLine"].as_str().unwrap_or("");
    Ok((!name.is_empty() && !tag.is_empty()).then(|| (name.to_string(), tag.to_string())))
}

/// Region code like "EUW" / "NA", falling back to the platform id ("EUW1").
pub async fn region(c: &LcuClient) -> Option<String> {
    if let Ok(v) = c.get("/riotclient/region-locale").await
        && let Some(r) = v["region"].as_str().filter(|r| !r.is_empty())
    {
        return Some(r.to_ascii_uppercase());
    }
    c.get("/lol-platform-config/v1/namespaces/LoginDataPacket/platformId")
        .await
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_ascii_uppercase()))
}

/// Client game version, e.g. "16.20.715.1234" -> patch "16.20".
pub async fn client_patch(c: &LcuClient) -> Option<String> {
    let v = c.get("/lol-patch/v1/game-version").await.ok()?;
    patch_of(v.as_str()?)
}

pub fn patch_of(version: &str) -> Option<String> {
    let mut it = version.split('.');
    let (a, b) = (it.next()?, it.next()?);
    (a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok()).then(|| format!("{a}.{b}"))
}

// ---------- game data (served locally by the client) ----------

pub async fn champion_summary(c: &LcuClient) -> LcuResult<Value> {
    c.get("/lol-game-data/assets/v1/champion-summary.json")
        .await
}

pub async fn champion_rune_recommendations(c: &LcuClient) -> LcuResult<Value> {
    c.get("/lol-game-data/assets/v1/champion-rune-recommendations.json")
        .await
}

// ---------- rune pages ----------

pub async fn perk_pages(c: &LcuClient) -> LcuResult<Vec<Value>> {
    Ok(c.get("/lol-perks/v1/pages")
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

pub async fn create_perk_page(c: &LcuClient, page: &Value) -> LcuResult<Value> {
    c.post("/lol-perks/v1/pages", page).await
}

pub async fn update_perk_page(c: &LcuClient, id: i64, page: &Value) -> LcuResult<Value> {
    c.put(&format!("/lol-perks/v1/pages/{id}"), page).await
}

pub async fn delete_perk_page(c: &LcuClient, id: i64) -> LcuResult<Value> {
    c.delete(&format!("/lol-perks/v1/pages/{id}")).await
}

pub async fn set_current_perk_page(c: &LcuClient, id: i64) -> LcuResult<Value> {
    c.put("/lol-perks/v1/currentpage", &json!(id)).await
}

// ---------- item sets ----------

pub async fn item_sets(c: &LcuClient, summoner_id: i64) -> LcuResult<Value> {
    c.get(&format!("/lol-item-sets/v1/item-sets/{summoner_id}/sets"))
        .await
}

pub async fn put_item_sets(c: &LcuClient, summoner_id: i64, body: &Value) -> LcuResult<Value> {
    c.put(
        &format!("/lol-item-sets/v1/item-sets/{summoner_id}/sets"),
        body,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../fixtures/champ-select-session.json")).unwrap()
    }

    #[test]
    fn selection_locked() {
        let s = parse_selection(&fixture()).unwrap();
        assert_eq!(s.local_cell, 2);
        assert_eq!(s.locked_champion, 103);
        assert!(s.is_locked());
        assert_eq!(s.current_champion(), 103);
        assert_eq!(s.position, "middle");
        assert_eq!(s.spells, (14, 4));
        assert_eq!(s.game_id, 7_000_000_001);
    }

    #[test]
    fn selection_hover_only() {
        let mut v = fixture();
        // Make my pick action not completed: champion is only hovered.
        for g in v["actions"].as_array_mut().unwrap() {
            for a in g.as_array_mut().unwrap() {
                if a["actorCellId"] == 2 {
                    a["completed"] = json!(false);
                }
            }
        }
        let s = parse_selection(&v).unwrap();
        assert!(!s.is_locked());
        assert_eq!(s.hover_champion, 103);
        assert_eq!(s.current_champion(), 103);
    }

    #[test]
    fn selection_no_pick_action() {
        let mut v = fixture();
        v["actions"] = json!([]);
        let s = parse_selection(&v).unwrap();
        assert_eq!(s.locked_champion, 103);
    }

    #[test]
    fn anonymised_allies_are_never_exposed() {
        let s = parse_selection(&fixture()).unwrap();
        let hidden = s.allies.iter().find(|a| a.cell_id == 3).unwrap();
        assert_eq!(hidden.riot_id, None);
        assert_eq!(hidden.visible_puuid, None);
        // Party member with visible name.
        let friend = s.allies.iter().find(|a| a.cell_id == 1).unwrap();
        assert_eq!(friend.riot_id, Some(("Duo Partner".into(), "EUW".into())));
        // Local player always visible.
        let me = s.allies.iter().find(|a| a.is_local).unwrap();
        assert!(me.riot_id.is_some());
        // Visibility unknown + puuid only: no puuid lookup.
        let unknown = s.allies.iter().find(|a| a.cell_id == 4).unwrap();
        assert_eq!(unknown.visible_puuid, None);
    }

    #[test]
    fn lobby_members_and_party_cells() {
        let lobby = json!({"members":[
            {"puuid":"puuid-me","gameName":"Me Myself","gameTag":"1234","isLeader":true},
            {"puuid":"puuid-friend","summonerId":111},
            {"puuid":""}
        ]});
        let party = parse_lobby_members(&lobby);
        assert_eq!(party.len(), 2);
        assert_eq!(party[0].riot_id, Some(("Me Myself".into(), "1234".into())));
        assert_eq!(party[1].riot_id, None);
        assert!(parse_lobby_members(&json!({"errorCode":"RPC_ERROR"})).is_empty());
        let mut cells = party_cells(&fixture(), &party);
        cells.sort();
        assert_eq!(cells, vec![1, 2]);
        // Hidden allies are never matched unless they are in your lobby.
        assert!(party_cells(&fixture(), &[]).is_empty());
    }

    #[test]
    fn not_in_champ_select() {
        assert!(parse_selection(&json!({"errorCode":"RPC_ERROR"})).is_none());
    }

    #[test]
    fn ready_check_decision() {
        assert!(should_accept(
            &json!({"state":"InProgress","playerResponse":"None"})
        ));
        assert!(!should_accept(
            &json!({"state":"InProgress","playerResponse":"Declined"})
        ));
        assert!(!should_accept(
            &json!({"state":"InProgress","playerResponse":"Accepted"})
        ));
        assert!(!should_accept(
            &json!({"state":"Invalid","playerResponse":"None"})
        ));
        assert!(!should_accept(&Value::Null));
    }

    #[test]
    fn map_queue_and_patch() {
        let s = json!({"map":{"id":11},"gameData":{"queue":{"id":420,"mapId":11}}});
        assert_eq!(parse_map_queue(&s), (Some(11), Some(420)));
        let s = json!({"gameData":{"queue":{"id":450,"mapId":12}}});
        assert_eq!(parse_map_queue(&s), (Some(12), Some(450)));
        assert_eq!(patch_of("16.20.715.1234").as_deref(), Some("16.20"));
        assert_eq!(patch_of("16.20.1").as_deref(), Some("16.20"));
        assert_eq!(patch_of("garbage"), None);
    }
}
