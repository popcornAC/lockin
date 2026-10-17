//! Icon lookup tables for the window (champion keys, summoner spell / rune
//! icon URLs, item names), built from Data Dragon (+ CommunityDragon for the
//! stat shards, which Data Dragon doesn't list) once per patch and cached on
//! disk. The images themselves are loaded by the webview, which caches them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DDRAGON: &str = "https://ddragon.leagueoflegends.com";
const CDRAGON_GAME_DATA: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1";
const FILE_PREFIX: &str = "ddragon-assets-";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Named {
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GameAssets {
    /// Full Data Dragon version, e.g. "16.20.1" (used in image URLs).
    pub version: String,
    /// Lookup key -> Data Dragon champion id ("MonkeyKing"). Keys: numeric
    /// champion id, and [`norm`] of both the display name and the id.
    pub champions: HashMap<String, String>,
    /// Champion display names by Data Dragon id.
    pub champion_names: HashMap<String, String>,
    pub spells: HashMap<i64, Named>,
    /// Rune trees, runes and stat shards by perk id.
    pub runes: HashMap<i64, Named>,
    /// Item names by id (icons follow from the id).
    pub items: HashMap<i64, String>,
}

/// Lower-case ASCII alphanumerics only: "Kai'Sa" -> "kaisa", "Nunu & Willump" -> "nunuwillump".
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn ddragon_img(version: &str, path: &str) -> String {
    format!("{DDRAGON}/cdn/{version}/img/{path}")
}

pub fn parse_champions(v: &Value, a: &mut GameAssets) {
    for (id, c) in v["data"].as_object().into_iter().flatten() {
        let name = c["name"].as_str().unwrap_or(id);
        if let Some(key) = c["key"].as_str() {
            a.champions.insert(key.to_string(), id.clone());
        }
        a.champions.insert(norm(id), id.clone());
        a.champions.insert(norm(name), id.clone());
        a.champion_names.insert(id.clone(), name.to_string());
    }
}

pub fn parse_spells(v: &Value, a: &mut GameAssets) {
    for s in v["data"].as_object().into_iter().flatten().map(|(_, s)| s) {
        let (Some(key), Some(img)) = (
            s["key"].as_str().and_then(|k| k.parse::<i64>().ok()),
            s["image"]["full"].as_str(),
        ) else {
            continue;
        };
        a.spells.insert(
            key,
            Named {
                name: s["name"].as_str().unwrap_or_default().to_string(),
                icon: ddragon_img(&a.version, &format!("spell/{img}")),
            },
        );
    }
}

pub fn parse_runes(v: &Value, a: &mut GameAssets) {
    let named = |r: &Value| Named {
        name: r["name"].as_str().unwrap_or_default().to_string(),
        icon: format!(
            "{DDRAGON}/cdn/img/{}",
            r["icon"].as_str().unwrap_or_default()
        ),
    };
    for style in v.as_array().into_iter().flatten() {
        if let Some(id) = style["id"].as_i64() {
            a.runes.insert(id, named(style));
        }
        for slot in style["slots"].as_array().into_iter().flatten() {
            for r in slot["runes"].as_array().into_iter().flatten() {
                if let Some(id) = r["id"].as_i64() {
                    a.runes.insert(id, named(r));
                }
            }
        }
    }
}

/// Stat shards from CommunityDragon's `perks.json` (ids 5000-5999).
pub fn parse_stat_shards(v: &Value, a: &mut GameAssets) {
    for p in v.as_array().into_iter().flatten() {
        let (Some(id), Some(path)) = (p["id"].as_i64(), p["iconPath"].as_str()) else {
            continue;
        };
        if !(5000..6000).contains(&id) {
            continue;
        }
        let Some(rest) = path.strip_prefix("/lol-game-data/assets/v1/") else {
            continue;
        };
        a.runes.insert(
            id,
            Named {
                name: p["name"].as_str().unwrap_or_default().to_string(),
                icon: format!("{CDRAGON_GAME_DATA}/{}", rest.to_lowercase()),
            },
        );
    }
}

pub fn parse_items(v: &Value, a: &mut GameAssets) {
    for (id, it) in v["data"].as_object().into_iter().flatten() {
        if let (Ok(id), Some(name)) = (id.parse::<i64>(), it["name"].as_str()) {
            a.items.insert(id, name.to_string());
        }
    }
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    http.get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch(http: &reqwest::Client, version: &str) -> Result<GameAssets, String> {
    let data = format!("{DDRAGON}/cdn/{version}/data/en_US");
    let (champs, spells, runes, items, perks) = futures_util::future::join5(
        get_json(http, &format!("{data}/champion.json")),
        get_json(http, &format!("{data}/summoner.json")),
        get_json(http, &format!("{data}/runesReforged.json")),
        get_json(http, &format!("{data}/item.json")),
        get_json(http, &format!("{CDRAGON_GAME_DATA}/perks.json")),
    )
    .await;
    let mut a = GameAssets {
        version: version.to_string(),
        ..Default::default()
    };
    parse_champions(&champs?, &mut a);
    parse_spells(&spells?, &mut a);
    parse_runes(&runes?, &mut a);
    // Optional extras: missing names / shard icons just show placeholders.
    if let Ok(v) = items {
        parse_items(&v, &mut a);
    }
    if let Ok(v) = perks {
        parse_stat_shards(&v, &mut a);
    }
    Ok(a)
}

fn cache_file(dir: &Path, version: &str) -> PathBuf {
    dir.join(format!("{FILE_PREFIX}{version}.json"))
}

fn read_file(path: &Path) -> Option<GameAssets> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Newest cached table on disk (for offline starts).
pub fn load_cached(dir: &Path) -> Option<GameAssets> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(FILE_PREFIX))
        })
        .collect();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    files.iter().rev().find_map(|p| read_file(p))
}

/// Tables for `version`: from the disk cache, else fetched and cached (older
/// patches' files are removed).
pub async fn load(http: &reqwest::Client, dir: &Path, version: &str) -> Result<GameAssets, String> {
    let file = cache_file(dir, version);
    if let Some(a) = read_file(&file) {
        return Ok(a);
    }
    let a = fetch(http, version).await?;
    let _ = std::fs::create_dir_all(dir);
    if let Ok(s) = serde_json::to_string(&a) {
        let _ = std::fs::write(&file, s);
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p != file
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(FILE_PREFIX))
            {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalises_names() {
        assert_eq!(norm("Kai'Sa"), "kaisa");
        assert_eq!(norm("Nunu & Willump"), "nunuwillump");
        assert_eq!(norm("MonkeyKing"), "monkeyking");
    }

    #[test]
    fn parses_tables() {
        let mut a = GameAssets {
            version: "16.20.1".into(),
            ..Default::default()
        };
        parse_champions(
            &json!({"data":{"MonkeyKing":{"key":"62","name":"Wukong"},"Kaisa":{"key":"145","name":"Kai'Sa"}}}),
            &mut a,
        );
        assert_eq!(a.champions["62"], "MonkeyKing");
        assert_eq!(a.champions["wukong"], "MonkeyKing");
        assert_eq!(a.champions["monkeyking"], "MonkeyKing");
        assert_eq!(a.champions["kaisa"], "Kaisa");
        assert_eq!(a.champion_names["Kaisa"], "Kai'Sa");

        parse_spells(
            &json!({"data":{"SummonerFlash":{"key":"4","name":"Flash","image":{"full":"SummonerFlash.png"}}}}),
            &mut a,
        );
        assert_eq!(
            a.spells[&4].icon,
            "https://ddragon.leagueoflegends.com/cdn/16.20.1/img/spell/SummonerFlash.png"
        );

        parse_runes(
            &json!([{"id":8100,"name":"Domination","icon":"perk-images/Styles/7200_Domination.png","slots":[{"runes":[{"id":8112,"name":"Electrocute","icon":"perk-images/Styles/Domination/Electrocute/Electrocute.png"}]}]}]),
            &mut a,
        );
        assert_eq!(a.runes[&8100].name, "Domination");
        assert_eq!(
            a.runes[&8112].icon,
            "https://ddragon.leagueoflegends.com/cdn/img/perk-images/Styles/Domination/Electrocute/Electrocute.png"
        );

        parse_stat_shards(
            &json!([
                {"id":5008,"name":"Adaptive Force","iconPath":"/lol-game-data/assets/v1/perk-images/StatMods/StatModsAdaptiveForceIcon.png"},
                {"id":8112,"name":"Electrocute","iconPath":"/lol-game-data/assets/v1/perk-images/x.png"}
            ]),
            &mut a,
        );
        assert_eq!(
            a.runes[&5008].icon,
            "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/perk-images/statmods/statmodsadaptiveforceicon.png"
        );
        assert_eq!(a.runes[&8112].name, "Electrocute");

        parse_items(
            &json!({"data":{"3157":{"name":"Zhonya's Hourglass"}}}),
            &mut a,
        );
        assert_eq!(a.items[&3157], "Zhonya's Hourglass");
    }
}
