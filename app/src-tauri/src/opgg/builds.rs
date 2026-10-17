//! OP.GG build source (`lol_get_champion_analysis`), cached per patch on disk.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};

use crate::import::client_recs::with_counts;
use crate::import::source::{
    BoxFuture, Build, BuildContext, BuildQuery, BuildSource, ItemBlock, RunePage,
};

/// Below this many games for the rune page we don't trust the sample.
pub const MIN_GAMES: u64 = 100;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Tier values accepted by OP.GG (verified against its validation error), in
/// the order offered in the UI.
pub const TIERS: [(&str, &str); 9] = [
    ("all", "All ranks"),
    ("gold_plus", "Gold+"),
    ("platinum_plus", "Platinum+"),
    ("emerald_plus", "Emerald+"),
    ("diamond_plus", "Diamond+"),
    ("master_plus", "Master+"),
    ("master", "Master"),
    ("grandmaster", "Grandmaster"),
    ("challenger", "Challenger"),
];
pub const DEFAULT_TIER: &str = "gold_plus";

pub fn tier_label(tier: &str) -> &'static str {
    TIERS
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, l)| *l)
        .unwrap_or("All ranks")
}

pub fn valid_tier(tier: &str) -> bool {
    TIERS.iter().any(|(t, _)| *t == tier)
}

const FIELDS: [&str; 10] = [
    "data.summary.positions[].name",
    "data.runes.{id,pick_rate,play,primary_page_id,primary_rune_ids[],secondary_page_id,secondary_rune_ids[],stat_mod_ids[],win}",
    "data.summoner_spells.{ids[],pick_rate,play,win}",
    "data.core_items.{ids[],pick_rate,play,win}",
    "data.starter_items.{ids[]}",
    "data.boots.{ids[]}",
    "data.fourth_items[].{ids[],pick_rate,play,win}",
    "data.fifth_items[].{ids[],pick_rate,play,win}",
    "data.sixth_items[].{ids[],pick_rate,play,win}",
    "data.skills.{order[]}",
];

/// "MonkeyKing" -> "MONKEY_KING", "JarvanIV" -> "JARVAN_IV", "Kaisa" -> "KAISA".
pub fn champion_param(alias: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in alias.chars().filter(|c| c.is_ascii_alphanumeric()) {
        if c.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// LCU position -> OP.GG position.
pub fn position_param(lcu: &str) -> Option<&'static str> {
    Some(match lcu {
        "top" => "top",
        "jungle" => "jungle",
        "middle" | "mid" => "mid",
        "bottom" | "adc" => "adc",
        "utility" | "support" => "support",
        _ => return None,
    })
}

/// OP.GG summary position name ("MID", "ADC", ...) -> OP.GG position param.
fn summary_position(name: &str) -> Option<&'static str> {
    position_param(&name.to_ascii_lowercase())
}

/// Ranked flex (queue 440) uses flex data; everything else on SR uses solo/duo.
pub fn game_mode(queue_id: Option<i64>) -> &'static str {
    if queue_id == Some(440) {
        "flex"
    } else {
        "ranked"
    }
}

pub fn analysis_args(champion: &str, position: &str, mode: &str, tier: &str) -> Value {
    let mut args = json!({
        "game_mode": mode,
        "champion": champion,
        "position": position,
        "desired_output_fields": FIELDS,
    });
    if tier != "all" {
        args["tier"] = json!(tier);
    }
    args
}

fn ids(v: &Value) -> Vec<i64> {
    v["ids"]
        .as_array()
        .map(|a| a.iter().filter_map(|i| i.as_i64()).collect())
        .unwrap_or_default()
}

/// "Q > W > E" from a level-by-level skill order: skills ranked by the level at
/// which they reach their last point in the listed order.
pub fn skill_priority(order: &[String]) -> Option<String> {
    let mut keys: Vec<(&str, usize, usize)> = ["Q", "W", "E"]
        .iter()
        .map(|k| {
            let n = order.iter().filter(|s| s.as_str() == *k).count();
            let fifth = order
                .iter()
                .enumerate()
                .filter(|(_, s)| s.as_str() == *k)
                .nth(4)
                .map(|(i, _)| i)
                .unwrap_or(usize::MAX);
            (*k, fifth, n)
        })
        .collect();
    if keys.iter().all(|(_, _, n)| *n == 0) {
        return None;
    }
    // Earlier max first; ties broken by more points.
    keys.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
    Some(
        keys.iter()
            .map(|(k, _, _)| *k)
            .collect::<Vec<_>>()
            .join(" > "),
    )
}

pub struct Analysis {
    pub build: Build,
    /// OP.GG's main position for the champion (param form, e.g. "mid").
    pub main_position: Option<&'static str>,
}

/// Map the parsed analysis tree to a [`Build`]. Errors if the core parts are missing.
pub fn build_from_analysis(v: &Value, source_label: String) -> Result<Analysis, String> {
    let d = &v["data"];
    if !d.is_object() {
        return Err("no data in OP.GG response".into());
    }
    let main_position = d["summary"]["positions"]
        .as_array()
        .and_then(|p| p.first())
        .and_then(|p| p["name"].as_str())
        .and_then(summary_position);

    let r = &d["runes"];
    let int_list = |k: &str| -> Vec<i64> {
        r[k].as_array()
            .map(|a| a.iter().filter_map(|i| i.as_i64()).collect())
            .unwrap_or_default()
    };
    let (prim, sec, stats) = (
        int_list("primary_rune_ids"),
        int_list("secondary_rune_ids"),
        int_list("stat_mod_ids"),
    );
    let runes = match (
        r["primary_page_id"].as_i64(),
        r["secondary_page_id"].as_i64(),
    ) {
        (Some(p), Some(s)) if prim.len() == 4 && sec.len() == 2 && stats.len() == 3 => {
            Some(RunePage {
                primary_style: p,
                sub_style: s,
                perks: prim.iter().chain(&sec).chain(&stats).copied().collect(),
            })
        }
        _ => None,
    };
    let games = r["play"].as_u64();
    let win_rate = match (r["win"].as_f64(), r["play"].as_f64()) {
        (Some(w), Some(p)) if p > 0.0 => Some(w / p),
        _ => None,
    };

    let spells = {
        let s = ids(&d["summoner_spells"]);
        (s.len() == 2).then(|| [s[0], s[1]])
    };

    let order: Vec<String> = d["skills"]["order"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let skill_priority = skill_priority(&order);

    let mut blocks = Vec::new();
    let mut push = |title: String, items: Vec<(i64, u32)>| {
        if !items.is_empty() {
            blocks.push(ItemBlock { title, items });
        }
    };
    push("Starter".into(), with_counts(&ids(&d["starter_items"])));
    push("Boots".into(), with_counts(&ids(&d["boots"])));
    let core_title = match &skill_priority {
        Some(p) => {
            let first: Vec<&str> = order.iter().take(3).map(String::as_str).collect();
            format!("Core · skills {p} (start {})", first.join("-"))
        }
        None => "Core".into(),
    };
    push(core_title, with_counts(&ids(&d["core_items"])));
    for (key, title) in [
        ("fourth_items", "4th item options"),
        ("fifth_items", "5th item options"),
        ("sixth_items", "6th item options"),
    ] {
        let mut opts: Vec<i64> = Vec::new();
        for o in d[key].as_array().into_iter().flatten() {
            for id in ids(o) {
                if !opts.contains(&id) {
                    opts.push(id);
                }
            }
        }
        push(title.into(), opts.into_iter().map(|i| (i, 1)).collect());
    }

    if runes.is_none() && spells.is_none() && blocks.is_empty() {
        return Err("OP.GG returned no build data".into());
    }
    Ok(Analysis {
        build: Build {
            source: source_label,
            runes,
            spells,
            item_blocks: blocks,
            skill_priority,
            skill_order: order,
            games,
            win_rate,
        },
        main_position,
    })
}

pub struct OpggSource {
    http: reqwest::Client,
    dir: PathBuf,
    pub tier: Mutex<String>,
    /// patch -> (cache key -> build)
    cache: Mutex<(String, HashMap<String, Build>)>,
}

impl OpggSource {
    pub fn new(http: reqwest::Client, dir: PathBuf, tier: String) -> Self {
        Self {
            http,
            dir,
            tier: Mutex::new(tier),
            cache: Mutex::new((String::new(), HashMap::new())),
        }
    }

    fn cache_file(&self, patch: &str) -> PathBuf {
        self.dir.join(format!("opgg-builds-{patch}.json"))
    }

    fn cache_get(&self, patch: &str, key: &str) -> Option<Build> {
        let mut c = self.cache.lock().unwrap();
        if c.0 != patch {
            // Load this patch's file (or start empty) and drop other patches' files.
            let map = std::fs::read_to_string(self.cache_file(patch))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            *c = (patch.to_string(), map);
            self.remove_old_files(patch);
        }
        c.1.get(key).cloned()
    }

    fn cache_put(&self, patch: &str, key: String, build: &Build) {
        let mut c = self.cache.lock().unwrap();
        if c.0 != patch {
            return;
        }
        c.1.insert(key, build.clone());
        if patch != "unknown" {
            let _ = std::fs::create_dir_all(&self.dir);
            if let Ok(s) = serde_json::to_string(&c.1) {
                let _ = std::fs::write(self.cache_file(patch), s);
            }
        }
    }

    fn remove_old_files(&self, patch: &str) {
        let keep = format!("opgg-builds-{patch}.json");
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with("opgg-builds-") && n != keep {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }

    /// "Refresh builds": forget everything cached for the current patch.
    pub fn clear(&self, patch: &str) {
        let mut c = self.cache.lock().unwrap();
        *c = (patch.to_string(), HashMap::new());
        let _ = std::fs::remove_file(self.cache_file(patch));
    }

    async fn fetch_one(
        &self,
        q: &BuildQuery,
        pos: &str,
        mode: &str,
        tier: &str,
    ) -> Result<Analysis, String> {
        let patch = q.patch.clone().unwrap_or_else(|| "unknown".into());
        let key = format!("{}|{pos}|{mode}|{tier}", q.champion_id);
        if let Some(b) = self.cache_get(&patch, &key) {
            return Ok(Analysis {
                build: b,
                main_position: None,
            });
        }
        let args = analysis_args(&champion_param(&q.champion_alias), pos, mode, tier);
        let tree = super::call_tool(&self.http, "lol_get_champion_analysis", args, TIMEOUT).await?;
        let label = format!(
            "OP.GG ({}{}) · {} · patch {patch}",
            tier_label(tier),
            if mode == "flex" { ", flex" } else { "" },
            crate::import::source::position_label(lcu_position(pos)),
        );
        let a = build_from_analysis(&tree, label)?;
        self.cache_put(&patch, key, &a.build);
        Ok(a)
    }
}

/// OP.GG position param -> LCU position.
fn lcu_position(p: &str) -> &'static str {
    match p {
        "top" => "top",
        "jungle" => "jungle",
        "mid" => "middle",
        "adc" => "bottom",
        "support" => "utility",
        _ => "",
    }
}

impl BuildSource for OpggSource {
    fn id(&self) -> &'static str {
        "opgg"
    }

    fn fetch<'a>(&'a self, ctx: &'a BuildContext<'a>) -> BoxFuture<'a, Result<Build, String>> {
        Box::pin(async move {
            let q = ctx.query;
            if q.map_id != 11 {
                return Err("OP.GG source only covers Summoner's Rift".into());
            }
            let tier = self.tier.lock().unwrap().clone();
            let mode = game_mode(q.queue_id);
            let assigned = position_param(&q.position);
            let guess = assigned
                .or_else(|| position_param(&q.fallback_position))
                .unwrap_or("mid");
            let mut a = self.fetch_one(q, guess, mode, &tier).await?;
            // Unassigned role and OP.GG says the champion mostly plays elsewhere: ask once more.
            if assigned.is_none()
                && let Some(main) = a.main_position.filter(|m| *m != guess)
            {
                a = self.fetch_one(q, main, mode, &tier).await?;
            }
            let games = a.build.games.unwrap_or(0);
            if games < MIN_GAMES {
                return Err(format!("OP.GG sample too small ({games} games)"));
            }
            Ok(a.build)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champion_names() {
        assert_eq!(champion_param("MonkeyKing"), "MONKEY_KING");
        assert_eq!(champion_param("JarvanIV"), "JARVAN_IV");
        assert_eq!(champion_param("Kaisa"), "KAISA");
        assert_eq!(champion_param("Nunu"), "NUNU");
        assert_eq!(champion_param("DrMundo"), "DR_MUNDO");
        assert_eq!(champion_param("KSante"), "KSANTE");
        assert_eq!(champion_param("TwistedFate"), "TWISTED_FATE");
        assert_eq!(champion_param("Chogath"), "CHOGATH");
    }

    #[test]
    fn positions_and_modes() {
        assert_eq!(position_param("bottom"), Some("adc"));
        assert_eq!(position_param("utility"), Some("support"));
        assert_eq!(position_param("middle"), Some("mid"));
        assert_eq!(position_param(""), None);
        assert_eq!(game_mode(Some(440)), "flex");
        assert_eq!(game_mode(Some(420)), "ranked");
        assert_eq!(game_mode(Some(430)), "ranked");
        assert_eq!(game_mode(None), "ranked");
        let a = analysis_args("AHRI", "mid", "ranked", "all");
        assert!(a.get("tier").is_none());
        assert_eq!(
            analysis_args("AHRI", "mid", "ranked", "gold_plus")["tier"],
            "gold_plus"
        );
    }

    #[test]
    fn skills() {
        let o: Vec<String> = "W Q E Q Q R Q W Q W R W W E E"
            .split(' ')
            .map(String::from)
            .collect();
        assert_eq!(skill_priority(&o).as_deref(), Some("Q > W > E"));
        assert_eq!(skill_priority(&[]), None);
    }

    #[test]
    fn analysis_to_build() {
        let tree =
            super::super::textfmt::parse(include_str!("../../fixtures/opgg-ahri-mid.txt")).unwrap();
        let a = build_from_analysis(&tree, "OP.GG".into()).unwrap();
        let b = a.build;
        assert_eq!(a.main_position, Some("mid"));
        assert_eq!(
            b.runes,
            Some(RunePage {
                primary_style: 8100,
                sub_style: 8200,
                perks: vec![8112, 8139, 8140, 8106, 8210, 8226, 5005, 5008, 5001],
            })
        );
        assert_eq!(b.spells, Some([4, 14]));
        assert_eq!(b.games, Some(225190));
        assert!((b.win_rate.unwrap() - 114145.0 / 225190.0).abs() < 1e-9);
        assert_eq!(b.skill_priority.as_deref(), Some("Q > W > E"));
        assert_eq!(b.skill_order.len(), 15);
        assert_eq!(b.skill_order[..3], ["W", "Q", "E"]);
        let titles: Vec<&str> = b.item_blocks.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles[0], "Starter");
        assert_eq!(b.item_blocks[0].items, vec![(1056, 1), (2003, 2)]);
        assert_eq!(titles[1], "Boots");
        assert!(titles[2].starts_with("Core · skills Q > W > E (start W-Q-E)"));
        assert_eq!(
            b.item_blocks[2].items,
            vec![(3118, 1), (4645, 1), (3157, 1)]
        );
        assert_eq!(
            &titles[3..],
            ["4th item options", "5th item options", "6th item options"]
        );
        assert_eq!(
            b.item_blocks[3].items,
            vec![(3089, 1), (3157, 1), (4645, 1)]
        );
    }

    #[test]
    fn analysis_missing_data() {
        assert!(build_from_analysis(&json!({}), "x".into()).is_err());
        assert!(build_from_analysis(&json!({"data":{}}), "x".into()).is_err());
        // Partial data: runes with wrong lengths are dropped, spells kept.
        let v = json!({"data":{"runes":{"primary_page_id":8100,"secondary_page_id":8200,"primary_rune_ids":[1],"secondary_rune_ids":[],"stat_mod_ids":[]},"summoner_spells":{"ids":[4,14]}}});
        let b = build_from_analysis(&v, "x".into()).unwrap().build;
        assert!(b.runes.is_none());
        assert_eq!(b.spells, Some([4, 14]));
    }
}
