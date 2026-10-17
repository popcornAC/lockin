//! The pluggable build-source interface. A source turns "champion X, role Y,
//! queue Z" into runes / spells / item blocks. Sources are tried in order and
//! each part (runes, spells, items) is taken from the first source that has it.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::lcu::LcuClient;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunePage {
    pub primary_style: i64,
    pub sub_style: i64,
    /// 9 ids in client order: keystone, 3 primary, 2 secondary, 3 stat shards.
    pub perks: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemBlock {
    pub title: String,
    /// (item id, count)
    pub items: Vec<(i64, u32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Build {
    /// Human-readable source label, e.g. "OP.GG (Gold+) · patch 16.20".
    pub source: String,
    pub runes: Option<RunePage>,
    pub spells: Option<[i64; 2]>,
    pub item_blocks: Vec<ItemBlock>,
    /// e.g. "Q > W > E"
    pub skill_priority: Option<String>,
    /// Level-by-level skill order ("Q", "W", ...), if the source has it.
    #[serde(default)]
    pub skill_order: Vec<String>,
    /// Games behind this build, if the source knows.
    pub games: Option<u64>,
    pub win_rate: Option<f64>,
}

/// What we're importing for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildQuery {
    pub champion_id: i64,
    /// Data Dragon / LCU alias, e.g. "MonkeyKing".
    pub champion_alias: String,
    pub champion_name: String,
    /// LCU position (top/jungle/middle/bottom/utility) or "" when unassigned.
    pub position: String,
    /// Best guess when `position` is empty (champion's usual role).
    pub fallback_position: String,
    pub map_id: i64,
    pub queue_id: Option<i64>,
    /// Current patch, e.g. "16.20" (Data Dragon), if known.
    pub patch: Option<String>,
}

pub struct BuildContext<'a> {
    pub query: &'a BuildQuery,
    pub lcu: &'a LcuClient,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait BuildSource: Send + Sync {
    /// Short id for logs, e.g. "opgg".
    fn id(&self) -> &'static str;
    fn fetch<'a>(&'a self, ctx: &'a BuildContext<'a>) -> BoxFuture<'a, Result<Build, String>>;
}

/// LCU position -> short display label.
pub fn position_label(pos: &str) -> &'static str {
    match pos {
        "top" => "Top",
        "jungle" => "Jungle",
        "middle" => "Mid",
        "bottom" => "ADC",
        "utility" => "Support",
        _ => "Any",
    }
}
