//! Champ select auto-import: gather a build from the configured sources and
//! write it to the client (rune page, summoner spells, item set).

pub mod client_recs;
pub mod payload;
pub mod source;

use std::sync::Arc;

use serde::Serialize;

use crate::lcu::{LcuClient, api};
use source::{Build, BuildContext, BuildQuery, BuildSource, ItemBlock, RunePage, position_label};

#[derive(Debug, Clone, Copy)]
pub struct Parts {
    pub runes: bool,
    pub spells: bool,
    pub items: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PartStatus {
    Applied,
    Off,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartResult {
    pub status: PartStatus,
    pub source: Option<String>,
    pub detail: String,
}

impl PartResult {
    fn off() -> Self {
        Self {
            status: PartStatus::Off,
            source: None,
            detail: "turned off".into(),
        }
    }
    fn unavailable(d: impl Into<String>) -> Self {
        Self {
            status: PartStatus::Unavailable,
            source: None,
            detail: d.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub champion_id: i64,
    pub champion_name: String,
    pub position: String,
    pub runes: PartResult,
    pub spells: PartResult,
    pub items: PartResult,
    pub skill_priority: Option<String>,
    pub games: Option<u64>,
    pub win_rate: Option<f64>,
    /// What was chosen, for the window to draw (whether or not it applied).
    pub rune_page: Option<RunePage>,
    /// In slot order (D, F) when set in champ select.
    pub spell_ids: Option<[i64; 2]>,
    pub item_blocks: Vec<ItemBlock>,
    pub skill_order: Vec<String>,
    /// Sources that failed or were skipped, with why (for the log).
    pub notes: Vec<String>,
}

/// The build parts chosen from the sources, each tagged with its source label.
#[derive(Debug, Default)]
pub struct Plan {
    pub runes: Option<(RunePage, String)>,
    pub spells: Option<([i64; 2], String)>,
    pub items: Option<(Vec<ItemBlock>, String)>,
    pub skill_priority: Option<String>,
    pub skill_order: Vec<String>,
    pub games: Option<u64>,
    pub win_rate: Option<f64>,
    pub notes: Vec<String>,
}

impl Plan {
    fn complete(&self, want: Parts) -> bool {
        (!want.runes || self.runes.is_some())
            && (!want.spells || self.spells.is_some())
            && (!want.items || self.items.is_some())
    }

    /// Fill missing parts from `b`.
    pub fn absorb(&mut self, b: Build) {
        let primary = self.runes.is_none() && self.spells.is_none() && self.items.is_none();
        if self.runes.is_none() {
            self.runes = b.runes.map(|r| (r, b.source.clone()));
        }
        if self.spells.is_none() {
            self.spells = b.spells.map(|s| (s, b.source.clone()));
        }
        if self.items.is_none() && !b.item_blocks.is_empty() {
            self.items = Some((b.item_blocks, b.source.clone()));
        }
        if self.skill_priority.is_none() {
            self.skill_priority = b.skill_priority;
            self.skill_order = b.skill_order;
        }
        if primary {
            self.games = b.games;
            self.win_rate = b.win_rate;
        }
    }
}

/// Ask sources in order until every wanted part is covered.
pub async fn gather(sources: &[Arc<dyn BuildSource>], ctx: &BuildContext<'_>, want: Parts) -> Plan {
    let mut plan = Plan::default();
    for s in sources {
        if plan.complete(want) {
            break;
        }
        match s.fetch(ctx).await {
            Ok(b) => plan.absorb(b),
            Err(e) => plan.notes.push(format!("{}: {e}", s.id())),
        }
    }
    plan
}

async fn apply_runes(lcu: &LcuClient, name: &str, page: &RunePage) -> Result<String, String> {
    let body = payload::rune_page_body(name, page);
    let pages = api::perk_pages(lcu).await.map_err(|e| e.to_string())?;
    let id = if let Some(id) = payload::find_owned_page(&pages) {
        let mut b = body.clone();
        b["id"] = id.into();
        match api::update_perk_page(lcu, id, &b).await {
            Ok(_) => id,
            Err(_) => {
                // Some client versions reject PUT; recreate our own page instead.
                let _ = api::delete_perk_page(lcu, id).await;
                create_page(lcu, &body).await?
            }
        }
    } else {
        create_page(lcu, &body).await?
    };
    let _ = api::set_current_perk_page(lcu, id).await;
    Ok(format!("rune page \"{name}\""))
}

async fn create_page(lcu: &LcuClient, body: &serde_json::Value) -> Result<i64, String> {
    let created = api::create_perk_page(lcu, body).await.map_err(|e| {
        format!("{e}. If your rune pages are full, delete one or rename one to start with \"{}\" so the app can reuse it", payload::PAGE_PREFIX)
    })?;
    created["id"]
        .as_i64()
        .ok_or_else(|| "client did not return the new page id".to_string())
}

async fn apply_items(
    lcu: &LcuClient,
    q: &BuildQuery,
    blocks: &[ItemBlock],
) -> Result<String, String> {
    let me = api::current_summoner(lcu)
        .await
        .map_err(|e| e.to_string())?;
    let existing = api::item_sets(lcu, me.summoner_id)
        .await
        .map_err(|e| e.to_string())?;
    let title = format!(
        "{} {} (Lockin)",
        q.champion_name,
        position_label(&q.position_or_fallback())
    );
    let mut doc =
        payload::merge_item_sets(&existing, payload::item_set(q.champion_id, &title, blocks));
    if doc.get("accountId").is_none() {
        doc["accountId"] = me.account_id.into();
    }
    api::put_item_sets(lcu, me.summoner_id, &doc)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("item set \"{title}\""))
}

impl BuildQuery {
    pub fn position_or_fallback(&self) -> String {
        if self.position.is_empty() {
            self.fallback_position.clone()
        } else {
            self.position.clone()
        }
    }
}

/// Apply the plan. `current_spells` is `Some` only while in champ select.
pub async fn apply(
    lcu: &LcuClient,
    q: &BuildQuery,
    plan: Plan,
    want: Parts,
    current_spells: Option<(i64, i64)>,
) -> ImportReport {
    let rune_page = plan.runes.as_ref().map(|(p, _)| p.clone());
    let spell_ids = plan.spells.as_ref().map(|(rec, _)| match current_spells {
        Some(cur) => payload::order_spells(*rec, cur),
        None => *rec,
    });
    let item_blocks = plan
        .items
        .as_ref()
        .map(|(b, _)| b.clone())
        .unwrap_or_default();
    let runes = if !want.runes {
        PartResult::off()
    } else if let Some((page, src)) = &plan.runes {
        let name = payload::page_name(&q.champion_name, position_label(&q.position_or_fallback()));
        match apply_runes(lcu, &name, page).await {
            Ok(d) => PartResult {
                status: PartStatus::Applied,
                source: Some(src.clone()),
                detail: d,
            },
            Err(e) => PartResult {
                status: PartStatus::Failed,
                source: Some(src.clone()),
                detail: e,
            },
        }
    } else {
        PartResult::unavailable("no rune data from any source")
    };

    let spells = if !want.spells {
        PartResult::off()
    } else if let Some((rec, src)) = plan.spells {
        match current_spells {
            None => PartResult::unavailable("summoner spells can only be set in champ select"),
            Some(cur) => {
                let [a, b] = payload::order_spells(rec, cur);
                match api::set_summoner_spells(lcu, a, b).await {
                    Ok(_) => PartResult {
                        status: PartStatus::Applied,
                        source: Some(src),
                        detail: format!("spells {a} + {b}"),
                    },
                    Err(e) => PartResult {
                        status: PartStatus::Failed,
                        source: Some(src),
                        detail: e.to_string(),
                    },
                }
            }
        }
    } else {
        PartResult::unavailable("no summoner spell data from any source")
    };

    let items = if !want.items {
        PartResult::off()
    } else if let Some((blocks, src)) = &plan.items {
        match apply_items(lcu, q, blocks).await {
            Ok(d) => PartResult {
                status: PartStatus::Applied,
                source: Some(src.clone()),
                detail: d,
            },
            Err(e) => PartResult {
                status: PartStatus::Failed,
                source: Some(src.clone()),
                detail: e,
            },
        }
    } else {
        PartResult::unavailable("no item data from any source")
    };

    ImportReport {
        champion_id: q.champion_id,
        champion_name: q.champion_name.clone(),
        position: q.position_or_fallback(),
        runes,
        spells,
        items,
        skill_priority: plan.skill_priority,
        games: plan.games,
        win_rate: plan.win_rate,
        rune_page,
        spell_ids,
        item_blocks,
        skill_order: plan.skill_order,
        notes: plan.notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(src: &str, runes: bool, spells: bool, items: bool) -> Build {
        Build {
            source: src.into(),
            runes: runes.then(|| RunePage {
                primary_style: 1,
                sub_style: 2,
                perks: vec![0; 9],
            }),
            spells: spells.then_some([4, 14]),
            item_blocks: if items {
                vec![ItemBlock {
                    title: "x".into(),
                    items: vec![(1, 1)],
                }]
            } else {
                vec![]
            },
            skill_priority: None,
            skill_order: vec![],
            games: Some(10),
            win_rate: None,
        }
    }

    #[test]
    fn plan_takes_each_part_from_first_source_that_has_it() {
        let mut p = Plan::default();
        p.absorb(build("opgg", true, false, false));
        p.absorb(build("client", true, true, true));
        assert_eq!(p.runes.unwrap().1, "opgg");
        assert_eq!(p.spells.unwrap().1, "client");
        assert_eq!(p.items.unwrap().1, "client");
        assert_eq!(p.games, Some(10));
    }

    #[test]
    fn plan_completeness_respects_toggles() {
        let mut p = Plan::default();
        p.absorb(build("a", true, false, false));
        assert!(p.complete(Parts {
            runes: true,
            spells: false,
            items: false
        }));
        assert!(!p.complete(Parts {
            runes: true,
            spells: true,
            items: false
        }));
    }
}
