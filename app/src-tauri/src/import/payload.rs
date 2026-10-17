//! Pure builders for the LCU payloads we write (rune page, item set, spells).

use serde_json::{Value, json};

use super::source::{ItemBlock, RunePage};

/// Name prefix that marks the single rune page this app owns.
pub const PAGE_PREFIX: &str = "[Lockin] ";
/// Item set uid prefix; one app-owned set per champion: `lockin-<championId>`.
pub const ITEM_SET_UID_PREFIX: &str = "lockin-";
const MAX_PAGE_NAME: usize = 25;

pub fn page_name(champion: &str, position_label: &str) -> String {
    let mut n = format!("{PAGE_PREFIX}{champion} {position_label}");
    if n.chars().count() > MAX_PAGE_NAME {
        n = n.chars().take(MAX_PAGE_NAME).collect();
    }
    n.trim_end().to_string()
}

pub fn rune_page_body(name: &str, page: &RunePage) -> Value {
    json!({
        "name": name,
        "primaryStyleId": page.primary_style,
        "subStyleId": page.sub_style,
        "selectedPerkIds": page.perks,
        "current": true,
    })
}

/// Id of the page we own (by name prefix), if any. Never matches the user's pages.
pub fn find_owned_page(pages: &[Value]) -> Option<i64> {
    pages
        .iter()
        .find(|p| {
            p["name"]
                .as_str()
                .is_some_and(|n| n.starts_with(PAGE_PREFIX))
                && p["isEditable"].as_bool() != Some(false)
        })
        .and_then(|p| p["id"].as_i64())
}

/// Order recommended spells so a spell the user already has keeps its slot
/// (e.g. Flash stays on D or F as the user prefers).
pub fn order_spells(rec: [i64; 2], current: (i64, i64)) -> [i64; 2] {
    let cur = [current.0, current.1];
    let mut out = [0i64; 2];
    for i in 0..2 {
        if rec.contains(&cur[i]) && cur[i] != 0 {
            out[i] = cur[i];
        }
    }
    for s in rec {
        if out.contains(&s) {
            continue;
        }
        if let Some(slot) = out.iter_mut().find(|x| **x == 0) {
            *slot = s;
        }
    }
    out
}

pub fn item_set_uid(champion_id: i64) -> String {
    format!("{ITEM_SET_UID_PREFIX}{champion_id}")
}

pub fn item_set(champion_id: i64, title: &str, blocks: &[ItemBlock]) -> Value {
    let blocks: Vec<Value> = blocks
        .iter()
        .map(|b| {
            json!({
                "type": b.title,
                "items": b.items.iter().map(|(id, count)| json!({ "id": id.to_string(), "count": count })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "uid": item_set_uid(champion_id),
        "title": title,
        "type": "custom",
        "map": "any",
        "mode": "any",
        "startedFrom": "blank",
        "sortrank": 0,
        "associatedChampions": [champion_id],
        "associatedMaps": [11],
        "preferredItemSlots": [],
        "blocks": blocks,
    })
}

/// Insert / replace our set in the account's item-set document, leaving every
/// other set (the user's own) untouched.
pub fn merge_item_sets(existing: &Value, set: Value) -> Value {
    let mut doc = if existing.is_object() {
        existing.clone()
    } else {
        json!({})
    };
    let uid = set["uid"].as_str().unwrap_or_default().to_string();
    let mut sets: Vec<Value> = doc["itemSets"].as_array().cloned().unwrap_or_default();
    sets.retain(|s| s["uid"].as_str() != Some(uid.as_str()));
    sets.push(set);
    doc["itemSets"] = Value::Array(sets);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_body_and_name() {
        let p = RunePage {
            primary_style: 8100,
            sub_style: 8200,
            perks: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        };
        let b = rune_page_body(&page_name("Ahri", "Mid"), &p);
        assert_eq!(b["name"], "[Lockin] Ahri Mid");
        assert_eq!(b["primaryStyleId"], 8100);
        assert_eq!(b["subStyleId"], 8200);
        assert_eq!(b["selectedPerkIds"].as_array().unwrap().len(), 9);
        assert_eq!(b["current"], true);
        assert!(page_name("Nunu & Willump", "Jungle").chars().count() <= 25);
    }

    #[test]
    fn owned_page_detection() {
        let pages = vec![
            json!({"id": 1, "name": "My page", "isEditable": true}),
            json!({"id": 2, "name": "RH not ours", "isEditable": true}),
            json!({"id": 50, "name": "[Lockin] Ahri Mid", "isEditable": true}),
        ];
        assert_eq!(find_owned_page(&pages), Some(50));
        assert_eq!(find_owned_page(&pages[..2]), None);
        let locked = vec![json!({"id": 9, "name": "[Lockin] x", "isEditable": false})];
        assert_eq!(find_owned_page(&locked), None);
    }

    #[test]
    fn spells_keep_slots() {
        // User has Flash on F (slot 2): keep it there.
        assert_eq!(order_spells([4, 14], (14, 4)), [14, 4]);
        assert_eq!(order_spells([4, 14], (12, 4)), [14, 4]);
        assert_eq!(order_spells([4, 14], (4, 12)), [4, 14]);
        assert_eq!(order_spells([4, 11], (7, 3)), [4, 11]);
        assert_eq!(order_spells([4, 11], (0, 0)), [4, 11]);
    }

    #[test]
    fn item_set_merge_keeps_user_sets() {
        let existing = json!({
            "accountId": 123,
            "timestamp": 1,
            "itemSets": [
                {"uid": "user-set-1", "title": "Mine"},
                {"uid": "lockin-103", "title": "old"}
            ]
        });
        let blocks = vec![ItemBlock {
            title: "Starter".into(),
            items: vec![(1056, 1), (2003, 2)],
        }];
        let set = item_set(103, "Ahri Mid (OP.GG)", &blocks);
        assert_eq!(
            set["blocks"][0]["items"][1],
            json!({"id": "2003", "count": 2})
        );
        let merged = merge_item_sets(&existing, set);
        let sets = merged["itemSets"].as_array().unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0]["uid"], "user-set-1");
        assert_eq!(sets[1]["title"], "Ahri Mid (OP.GG)");
        assert_eq!(merged["accountId"], 123);
        // Empty / missing document.
        let m = merge_item_sets(&Value::Null, item_set(1, "x", &[]));
        assert_eq!(m["itemSets"].as_array().unwrap().len(), 1);
    }
}
