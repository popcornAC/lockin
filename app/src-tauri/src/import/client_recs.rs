//! Fallback build source: the League client's own recommendations.
//!
//! * Runes + summoner spells: `champion-rune-recommendations.json`, the data the
//!   client's "recommended pages" UI is built from. Served locally by the client
//!   at `/lol-game-data/assets/v1/...`; CommunityDragon's copy of the same file
//!   is the fallback.
//! * Item blocks: the in-game shop's recommended items, from the champion's
//!   game data (`ItemRecommendationOverrideSet` in `<champ>.bin.json` on
//!   CommunityDragon). Not served by the LCU, so this part needs the internet.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::source::{BoxFuture, Build, BuildContext, BuildSource, ItemBlock, RunePage};
use crate::lcu::api;

const CDRAGON: &str = "https://raw.communitydragon.org/latest";

pub struct ClientRecommendations {
    http: reqwest::Client,
    recs: Mutex<Option<Arc<Value>>>,
    bins: Mutex<HashMap<String, Arc<Value>>>,
}

impl ClientRecommendations {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            recs: Mutex::new(None),
            bins: Mutex::new(HashMap::new()),
        }
    }

    /// Rune recommendation list (cached for the app's lifetime).
    pub async fn recommendations(&self, lcu: &crate::lcu::LcuClient) -> Result<Arc<Value>, String> {
        if let Some(r) = self.recs.lock().unwrap().clone() {
            return Ok(r);
        }
        let v = match api::champion_rune_recommendations(lcu).await {
            Ok(v) if v.is_array() => v,
            _ => self
                .http
                .get(format!("{CDRAGON}/plugins/rcp-be-lol-game-data/global/default/v1/champion-rune-recommendations.json"))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.to_string())?
                .json::<Value>()
                .await
                .map_err(|e| e.to_string())?,
        };
        let v = Arc::new(v);
        *self.recs.lock().unwrap() = Some(v.clone());
        Ok(v)
    }

    async fn champion_bin(&self, alias: &str) -> Result<Arc<Value>, String> {
        let key = alias.to_ascii_lowercase();
        if let Some(b) = self.bins.lock().unwrap().get(&key).cloned() {
            return Ok(b);
        }
        let url = format!("{CDRAGON}/game/data/characters/{key}/{key}.bin.json");
        let v: Value = self
            .http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        // Keep only the part we need; bins can be large.
        let slim = Arc::new(Value::Array(
            find_item_override_sets(&v).into_iter().cloned().collect(),
        ));
        self.bins.lock().unwrap().insert(key, slim.clone());
        Ok(slim)
    }
}

impl BuildSource for ClientRecommendations {
    fn id(&self) -> &'static str {
        "client"
    }

    fn fetch<'a>(&'a self, ctx: &'a BuildContext<'a>) -> BoxFuture<'a, Result<Build, String>> {
        Box::pin(async move {
            let q = ctx.query;
            let mut build = Build {
                source: "Client recommendations".into(),
                ..Default::default()
            };
            let mut errors = Vec::new();
            match self.recommendations(ctx.lcu).await {
                Ok(recs) => {
                    let pos = if q.position.is_empty() {
                        &q.fallback_position
                    } else {
                        &q.position
                    };
                    if let Some(rec) = select_rune_rec(&recs, q.champion_id, pos, q.map_id) {
                        build.runes = rec_runes(rec);
                        build.spells = rec_spells(rec);
                    }
                }
                Err(e) => errors.push(format!("rune recommendations: {e}")),
            }
            match self.champion_bin(&q.champion_alias).await {
                Ok(sets) => {
                    let pos = if q.position.is_empty() {
                        &q.fallback_position
                    } else {
                        &q.position
                    };
                    build.item_blocks = item_blocks_from_override_sets(&sets, q.map_id, pos);
                }
                Err(e) => errors.push(format!("item recommendations: {e}")),
            }
            if build.runes.is_none() && build.spells.is_none() && build.item_blocks.is_empty() {
                return Err(if errors.is_empty() {
                    "no recommendation for this champion".into()
                } else {
                    errors.join("; ")
                });
            }
            Ok(build)
        })
    }
}

// ---------- pure helpers ----------

fn champion_recs(recs: &Value, champion_id: i64) -> Vec<&Value> {
    recs.as_array()
        .and_then(|a| {
            a.iter()
                .find(|c| c["championId"].as_i64() == Some(champion_id))
        })
        .and_then(|c| c["runeRecommendations"].as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Pick the recommendation for this champion/position/map: exact position
/// first, then the champion's default position on that map, then anything on
/// that map.
pub fn select_rune_rec<'a>(
    recs: &'a Value,
    champion_id: i64,
    position: &str,
    map_id: i64,
) -> Option<&'a Value> {
    let on_map: Vec<&Value> = champion_recs(recs, champion_id)
        .into_iter()
        .filter(|r| r["mapId"].as_i64() == Some(map_id))
        .collect();
    if !position.is_empty()
        && let Some(r) = on_map.iter().find(|r| {
            r["position"]
                .as_str()
                .is_some_and(|p| p.eq_ignore_ascii_case(position))
        })
    {
        return Some(r);
    }
    on_map
        .iter()
        .find(|r| r["isDefaultPosition"].as_bool() == Some(true))
        .or(on_map.first())
        .copied()
}

/// The champion's usual Summoner's Rift position (lower-case LCU style).
pub fn default_position(recs: &Value, champion_id: i64) -> Option<String> {
    champion_recs(recs, champion_id)
        .into_iter()
        .filter(|r| r["mapId"].as_i64() == Some(11))
        .find(|r| r["isDefaultPosition"].as_bool() == Some(true))
        .and_then(|r| r["position"].as_str())
        .filter(|p| !p.eq_ignore_ascii_case("NONE"))
        .map(|p| p.to_ascii_lowercase())
}

pub fn rec_runes(rec: &Value) -> Option<RunePage> {
    let perks: Vec<i64> = rec["perkIds"]
        .as_array()?
        .iter()
        .filter_map(|p| p.as_i64())
        .collect();
    (perks.len() == 9).then_some(RunePage {
        primary_style: rec["primaryPerkStyleId"].as_i64()?,
        sub_style: rec["secondaryPerkStyleId"].as_i64()?,
        perks,
    })
}

pub fn rec_spells(rec: &Value) -> Option<[i64; 2]> {
    let s = rec["summonerSpellIds"].as_array()?;
    Some([s.first()?.as_i64()?, s.get(1)?.as_i64()?])
}

fn find_item_override_sets(bin: &Value) -> Vec<&Value> {
    bin.as_object()
        .map(|m| {
            m.values()
                .filter(|v| v["__type"].as_str() == Some("ItemRecommendationOverrideSet"))
                .collect()
        })
        .unwrap_or_default()
}

fn item_ids(list: &Value) -> Vec<i64> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str()?.strip_prefix("Items/")?.parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Collapse duplicates into counts, keeping first-seen order.
pub fn with_counts(ids: &[i64]) -> Vec<(i64, u32)> {
    let mut out: Vec<(i64, u32)> = Vec::new();
    for id in ids {
        match out.iter_mut().find(|(i, _)| i == id) {
            Some((_, c)) => *c += 1,
            None => out.push((*id, 1)),
        }
    }
    out
}

/// Item blocks from the `ItemRecommendationOverrideSet`s (as a JSON array of
/// those objects) for Summoner's Rift and the given position.
pub fn item_blocks_from_override_sets(sets: &Value, map_id: i64, position: &str) -> Vec<ItemBlock> {
    let overrides: Vec<&Value> = sets
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["mOverrides"].as_array())
        .flatten()
        .collect();
    let ctx_matches = |o: &&Value, pos: Option<&str>| {
        o["mOverrideContexts"].as_array().is_some_and(|cs| {
            cs.iter().any(|c| {
                c["mMapID"].as_i64() == Some(map_id)
                    && c["mModeNameStringId"]
                        .as_str()
                        .is_none_or(|m| m == "CLASSIC")
                    && match pos {
                        Some(p) => c["mPosition"]
                            .as_str()
                            .is_some_and(|x| x.eq_ignore_ascii_case(p)),
                        None => c.get("mPosition").is_none(),
                    }
            })
        })
    };
    let specific = (!position.is_empty())
        .then(|| overrides.iter().find(|o| ctx_matches(o, Some(position))))
        .flatten();
    let base = overrides.iter().find(|o| ctx_matches(o, None));

    let pick = |field: &str| -> Option<&Value> {
        specific
            .and_then(|o| {
                o.get(field)
                    .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
            })
            .or_else(|| base.and_then(|o| o.get(field)))
    };

    let mut blocks = Vec::new();
    if let Some(bundles) = pick("StartingItemBundles").and_then(|b| b.as_array()) {
        let mut alts: Vec<i64> = Vec::new();
        for (i, b) in bundles.iter().enumerate() {
            let ids = item_ids(&b["items"]);
            if i == 0 {
                if !ids.is_empty() {
                    blocks.push(ItemBlock {
                        title: "Starter".into(),
                        items: with_counts(&ids),
                    });
                }
            } else {
                for id in ids {
                    if !alts.contains(&id) {
                        alts.push(id);
                    }
                }
            }
        }
        if !alts.is_empty() {
            blocks.push(ItemBlock {
                title: "Starter (alternatives)".into(),
                items: alts.into_iter().map(|i| (i, 1)).collect(),
            });
        }
    }
    if let Some(ranges) = pick("mRecItemRanges").and_then(|r| r.as_array()) {
        for (i, r) in ranges.iter().enumerate() {
            let ids = item_ids(&r["items"]);
            if !ids.is_empty() {
                blocks.push(ItemBlock {
                    title: format!("Build step {}", i + 1),
                    items: ids.into_iter().map(|i| (i, 1)).collect(),
                });
            }
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs() -> Value {
        serde_json::from_str(include_str!(
            "../../fixtures/champion-rune-recommendations.json"
        ))
        .unwrap()
    }

    #[test]
    fn rune_rec_by_position() {
        let r = recs();
        let rec = select_rune_rec(&r, 103, "middle", 11).unwrap();
        assert_eq!(rec["position"], "MIDDLE");
        let page = rec_runes(rec).unwrap();
        assert_eq!(page.primary_style, 8100);
        assert_eq!(page.sub_style, 8200);
        assert_eq!(
            page.perks,
            vec![8112, 8139, 8140, 8106, 8226, 8210, 5005, 5008, 5001]
        );
        assert_eq!(rec_spells(rec), Some([4, 14]));
    }

    #[test]
    fn rune_rec_fallbacks() {
        let r = recs();
        // Off-role: falls back to the default position on that map.
        let rec = select_rune_rec(&r, 103, "jungle", 11).unwrap();
        assert_eq!(rec["position"], "MIDDLE");
        // Unknown champion / map.
        assert!(select_rune_rec(&r, 999_999, "middle", 11).is_none());
        assert!(select_rune_rec(&r, 103, "middle", 30).is_none());
        assert_eq!(default_position(&r, 103).as_deref(), Some("middle"));
        assert_eq!(default_position(&r, 266).as_deref(), Some("top"));
    }

    #[test]
    fn item_blocks_from_bin() {
        let bin: Value =
            serde_json::from_str(include_str!("../../fixtures/ahri.bin.json")).unwrap();
        let sets = Value::Array(find_item_override_sets(&bin).into_iter().cloned().collect());
        let mid = item_blocks_from_override_sets(&sets, 11, "middle");
        assert_eq!(mid[0].title, "Starter");
        assert_eq!(mid[0].items, vec![(1056, 1), (2003, 2)]);
        assert_eq!(mid[1].title, "Starter (alternatives)");
        assert!(mid.iter().any(|b| b.items.iter().any(|(i, _)| *i == 6655)));
        // Support override has its own starter (support item).
        let sup = item_blocks_from_override_sets(&sets, 11, "utility");
        assert_eq!(sup[0].items[0].0, 3850);
        // No mode-specific (Arena-style 77xxxx) items leak into SR.
        assert!(
            mid.iter()
                .all(|b| b.items.iter().all(|(i, _)| *i < 100_000))
        );
        // Other maps get nothing from SR data.
        assert!(item_blocks_from_override_sets(&sets, 30, "").is_empty());
    }

    #[test]
    fn counts() {
        assert_eq!(with_counts(&[1, 2, 2, 3, 2]), vec![(1, 1), (2, 3), (3, 1)]);
    }
}
