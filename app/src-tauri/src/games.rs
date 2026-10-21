//! Post-game stats: the local store of finished games and the OP.GG sync.
//!
//! Layout under `<app data>/games/`: `index.json` (account, per-game list
//! summaries, games still waiting for OP.GG) and one `<id>.json` record per
//! game. Only the index is kept in memory; a record is read when it's opened.
//! The newest [`MAX_GAMES`] games are kept.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::opgg::games::{self, GameRecord, GameRef, GameSummary, RECORD_VERSION};

pub const MAX_GAMES: usize = 100;
/// Raw end-of-game payloads kept in `<app data>/debug/` for a future switch to
/// the local source.
pub const MAX_EOG_DUMPS: usize = 10;
/// Minutes after the game ends to look for it on OP.GG (cumulative 2, 6, 14,
/// 30 and 45 min). After the last try the pending row is dropped; a later
/// backfill still picks the game up.
pub const RETRY_AFTER_MIN: [i64; 5] = [2, 6, 14, 30, 45];
/// Game details fetched in parallel during a sync.
const DETAIL_CONCURRENCY: usize = 3;
/// A backfill from opening the Games tab is skipped if one ran this recently.
const BACKFILL_EVERY: Duration = Duration::from_secs(90);
/// How far OP.GG's end time may be from when the client showed the
/// end-of-game screen (normally under a minute earlier).
const MATCH_BEFORE_S: i64 = 15 * 60;
const MATCH_AFTER_S: i64 = 3 * 60;

/// Queues we follow on the client side: draft, ranked solo, blind, flex, quickplay.
pub const SR_QUEUES: [i64; 5] = [400, 420, 430, 440, 490];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub game_name: String,
    pub tag_line: String,
    /// OP.GG region code.
    pub region: String,
}

impl Account {
    pub fn riot_id(&self) -> String {
        format!("{}#{}", self.game_name, self.tag_line)
    }
}

/// A game that ended in the client and isn't on OP.GG yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub lcu_game_id: i64,
    /// Unix seconds when the client reached the end-of-game phase.
    pub ended_at: i64,
    pub queue_id: Option<i64>,
    pub champion_id: Option<i64>,
    pub account: Account,
}

impl Pending {
    pub fn matches(&self, g: &GameRef) -> bool {
        let end = g.ended_unix();
        g.supported()
            && end >= self.ended_at - MATCH_BEFORE_S
            && end <= self.ended_at + MATCH_AFTER_S
            && match (self.champion_id, g.champion_id) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Index {
    account: Option<Account>,
    /// Newest first.
    games: Vec<GameSummary>,
    pending: Vec<Pending>,
}

/// What the window shows in the Games tab.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GamesView {
    pub games: Vec<GameSummary>,
    pub pending: Vec<Pending>,
    pub syncing: bool,
    pub error: Option<String>,
    /// Whose games these are ("Name#TAG"), once known.
    pub account: Option<String>,
}

/// Write via a temp file and rename, so a crash never leaves half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// File name for a game id (OP.GG ids are base64url-ish; keep it filename-safe).
fn file_name(id: &str) -> String {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(120)
        .collect();
    format!("{safe}.json")
}

pub struct GameStore {
    dir: PathBuf,
    index: Mutex<Index>,
}

impl GameStore {
    pub fn open(dir: PathBuf) -> Self {
        let index = std::fs::read(dir.join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
            .unwrap_or_default();
        Self {
            dir,
            index: Mutex::new(index),
        }
    }

    fn save(&self, idx: &Index) {
        if let Ok(b) = serde_json::to_vec(idx) {
            let _ = write_atomic(&self.dir.join("index.json"), &b);
        }
    }

    fn edit<T>(&self, f: impl FnOnce(&mut Index) -> T) -> T {
        let mut idx = self.index.lock().unwrap();
        let r = f(&mut idx);
        self.save(&idx);
        r
    }

    pub fn account(&self) -> Option<Account> {
        self.index.lock().unwrap().account.clone()
    }

    /// Returns true if it changed.
    pub fn set_account(&self, a: Account) -> bool {
        if self.account().as_ref() == Some(&a) {
            return false;
        }
        self.edit(|i| i.account = Some(a));
        true
    }

    pub fn summaries(&self) -> Vec<GameSummary> {
        self.index.lock().unwrap().games.clone()
    }

    pub fn pending(&self) -> Vec<Pending> {
        self.index.lock().unwrap().pending.clone()
    }

    pub fn has(&self, id: &str) -> bool {
        self.index.lock().unwrap().games.iter().any(|g| g.id == id)
    }

    /// Read one stored game. Records from an older format are treated as missing.
    pub fn get(&self, id: &str) -> Option<GameRecord> {
        if !self.has(id) {
            return None;
        }
        let b = std::fs::read(self.dir.join(file_name(id))).ok()?;
        serde_json::from_slice::<GameRecord>(&b)
            .ok()
            .filter(|r| r.version == RECORD_VERSION)
    }

    /// Store a game and prune to the newest [`MAX_GAMES`]. Returns false if
    /// the record couldn't be written.
    pub fn insert(&self, rec: &GameRecord) -> bool {
        let Ok(bytes) = serde_json::to_vec(rec) else {
            return false;
        };
        if write_atomic(&self.dir.join(file_name(&rec.id)), &bytes).is_err() {
            return false;
        }
        let dropped = self.edit(|i| {
            i.games.retain(|g| g.id != rec.id);
            i.games.push(rec.summary());
            i.games.sort_by_key(|g| std::cmp::Reverse(g.ended_at));
            if i.games.len() > MAX_GAMES {
                i.games.split_off(MAX_GAMES)
            } else {
                Vec::new()
            }
        });
        for g in dropped {
            let _ = std::fs::remove_file(self.dir.join(file_name(&g.id)));
        }
        true
    }

    /// Add a pending game (once per client game id).
    pub fn add_pending(&self, p: Pending) -> bool {
        self.edit(|i| {
            if i.pending.iter().any(|x| x.lcu_game_id == p.lcu_game_id) {
                return false;
            }
            i.pending.push(p);
            true
        })
    }

    pub fn remove_pending(&self, lcu_game_id: i64) {
        self.edit(|i| i.pending.retain(|p| p.lcu_game_id != lcu_game_id));
    }

    pub fn is_pending(&self, lcu_game_id: i64) -> bool {
        self.index
            .lock()
            .unwrap()
            .pending
            .iter()
            .any(|p| p.lcu_game_id == lcu_game_id)
    }
}

/// Save a raw end-of-game payload, keeping the newest [`MAX_EOG_DUMPS`].
pub fn save_eog_dump(data_dir: &Path, game_id: i64, payload: &serde_json::Value) {
    let dir = data_dir.join("debug");
    let Ok(bytes) = serde_json::to_vec_pretty(payload) else {
        return;
    };
    if write_atomic(&dir.join(format!("eog-{game_id}.json")), &bytes).is_err() {
        return;
    }
    let mut dumps: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("eog-") && n.ends_with(".json")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    dumps.sort_by_key(|d| std::cmp::Reverse(d.0));
    for (_, p) in dumps.into_iter().skip(MAX_EOG_DUMPS) {
        let _ = std::fs::remove_file(p);
    }
}

/// The OP.GG side: keeps the store in step with the account's match list.
pub struct Games {
    pub store: GameStore,
    http: reqwest::Client,
    sync_lock: tokio::sync::Mutex<()>,
    last_sync: Mutex<Option<(Instant, Account)>>,
    pub syncing: Mutex<bool>,
    pub error: Mutex<Option<String>>,
}

impl Games {
    pub fn new(http: reqwest::Client, data_dir: &Path) -> Self {
        Self {
            store: GameStore::open(data_dir.join("games")),
            http,
            sync_lock: tokio::sync::Mutex::new(()),
            last_sync: Mutex::new(None),
            syncing: Mutex::new(false),
            error: Mutex::new(None),
        }
    }

    pub fn view(&self) -> GamesView {
        GamesView {
            games: self.store.summaries(),
            pending: self.store.pending(),
            syncing: *self.syncing.lock().unwrap(),
            error: self.error.lock().unwrap().clone(),
            account: self.store.account().map(|a| a.riot_id()),
        }
    }

    /// True if a backfill for this account ran within [`BACKFILL_EVERY`].
    pub fn recently_synced(&self, a: &Account) -> bool {
        self.last_sync
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(t, acct)| acct == a && t.elapsed() < BACKFILL_EVERY)
    }

    /// Fetch the match list once and store every new Summoner's Rift game.
    /// `on_progress` runs after each stored game so the list fills in.
    /// Returns how many games were added.
    pub async fn sync(&self, a: &Account, on_progress: impl Fn()) -> Result<usize, String> {
        let _g = self.sync_lock.lock().await;
        let list = games::fetch_list(&self.http, &a.game_name, &a.tag_line, &a.region).await?;
        *self.last_sync.lock().unwrap() = Some((Instant::now(), a.clone()));
        let riot_id = a.riot_id();
        let mut added = 0;
        let mut first_err = None;
        let new: Vec<&GameRef> = list
            .iter()
            .filter(|g| g.supported() && !self.store.has(&g.id))
            .collect();
        // A few at a time: quick first backfill without hammering OP.GG.
        for chunk in new.chunks(DETAIL_CONCURRENCY) {
            let results = futures_util::future::join_all(
                chunk
                    .iter()
                    .map(|g| games::fetch_detail(&self.http, &a.region, g, &riot_id)),
            )
            .await;
            for r in results {
                match r {
                    Ok(rec) => {
                        if self.store.insert(&rec) {
                            added += 1;
                        }
                    }
                    Err(e) => {
                        first_err.get_or_insert(e);
                    }
                }
            }
            on_progress();
        }
        // A pending game is done once a stored game matches it.
        let resolved: Vec<i64> = self
            .store
            .pending()
            .into_iter()
            .filter(|p| p.account == *a)
            .filter(|p| list.iter().any(|g| p.matches(g) && self.store.has(&g.id)))
            .map(|p| p.lcu_game_id)
            .collect();
        for id in &resolved {
            self.store.remove_pending(*id);
        }
        match first_err {
            Some(e) if added == 0 => Err(e),
            _ => Ok(added),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opgg::games::{PlayerRecord, TeamRecord};

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "lockin-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn rec(i: i64) -> GameRecord {
        let p = |side: &str| PlayerRecord {
            riot_id: format!("P{i}#{side}"),
            side: side.into(),
            position: "middle".into(),
            champion_id: 103,
            champion: "Ahri".into(),
            level: 16,
            kills: 5,
            deaths: 2,
            assists: 7,
            cs: 200,
            gold: 12000,
            damage: 20000,
            damage_taken: 15000,
            wards_placed: 9,
            control_wards: 2,
            largest_multi_kill: 1,
            items: vec![3118],
            role_item: None,
            spells: vec![4, 14],
            primary_style: Some(8100),
            keystone: Some(8112),
            sub_style: Some(8200),
            op_score: Some(7.1),
            op_rank: Some(2),
        };
        let t = |side: &str, win| TeamRecord {
            side: side.into(),
            win,
            kills: 20,
            dragons: 2,
            barons: 1,
            towers: 7,
            gold: 60000,
            bans: vec![],
        };
        GameRecord {
            version: RECORD_VERSION,
            id: format!("m_game{i}="),
            region: "EUW".into(),
            ended_at: 1_790_000_000 + i * 3600,
            duration: 1800,
            queue: "SOLORANKED".into(),
            average_tier: None,
            teams: vec![t("BLUE", true), t("RED", false)],
            players: vec![p("BLUE"), p("RED")],
            me: Some(0),
        }
    }

    fn game_files(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != "index.json")
            .count()
    }

    #[test]
    fn prunes_to_newest_100() {
        let dir = tmp_dir("prune");
        let s = GameStore::open(dir.clone());
        // Inserted out of order on purpose.
        for i in (0..60).chain(60..105).rev() {
            assert!(s.insert(&rec(i)));
        }
        let list = s.summaries();
        assert_eq!(list.len(), MAX_GAMES);
        assert_eq!(game_files(&dir), MAX_GAMES);
        // Newest first; the 5 oldest are gone, files included.
        assert_eq!(list[0].id, "m_game104=");
        assert_eq!(list[99].id, "m_game5=");
        assert!(list.windows(2).all(|w| w[0].ended_at > w[1].ended_at));
        assert!(s.get("m_game4=").is_none());
        assert!(!dir.join(file_name("m_game4=")).exists());
        // An older game than everything kept is dropped straight away.
        s.insert(&rec(-1));
        assert_eq!(s.summaries().len(), MAX_GAMES);
        assert!(!s.has("m_game-1="));
        // Re-inserting an existing game doesn't duplicate it.
        s.insert(&rec(50));
        assert_eq!(s.summaries().len(), MAX_GAMES);
        // Survives a restart; details load from disk on demand.
        let s2 = GameStore::open(dir.clone());
        assert_eq!(s2.summaries(), list);
        assert_eq!(s2.get("m_game104="), Some(rec(104)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_survives_restart_and_dedupes() {
        let dir = tmp_dir("pending");
        let acct = Account {
            game_name: "Someone".into(),
            tag_line: "EUW".into(),
            region: "EUW".into(),
        };
        let s = GameStore::open(dir.clone());
        assert!(s.set_account(acct.clone()));
        assert!(!s.set_account(acct.clone()));
        let p = Pending {
            lcu_game_id: 42,
            ended_at: 1_790_001_800,
            queue_id: Some(420),
            champion_id: Some(103),
            account: acct.clone(),
        };
        assert!(s.add_pending(p.clone()));
        assert!(!s.add_pending(p.clone()));
        let s2 = GameStore::open(dir.clone());
        assert_eq!(s2.pending(), vec![p]);
        assert_eq!(s2.account(), Some(acct));
        s2.remove_pending(42);
        assert!(!GameStore::open(dir.clone()).is_pending(42));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_record_versions_are_ignored() {
        let dir = tmp_dir("version");
        let s = GameStore::open(dir.clone());
        let mut r = rec(1);
        r.version = RECORD_VERSION + 1;
        s.insert(&r);
        assert!(s.has(&r.id));
        assert!(s.get(&r.id).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn matches_pending_by_end_time_and_champion() {
        let acct = Account {
            game_name: "a".into(),
            tag_line: "b".into(),
            region: "KR".into(),
        };
        let g = GameRef {
            id: "x".into(),
            created_at: String::new(),
            created_unix: 1_000_000,
            game_map: "SUMMONERS_RIFT".into(),
            game_type: "SOLORANKED".into(),
            duration: 1800,
            champion_id: Some(103),
        };
        let p = |ended_at, champion_id| Pending {
            lcu_game_id: 1,
            ended_at,
            queue_id: Some(420),
            champion_id,
            account: acct.clone(),
        };
        // Client end screen a little after OP.GG's end time.
        assert!(p(1_000_040, Some(103)).matches(&g));
        assert!(p(1_000_040, None).matches(&g));
        assert!(!p(1_000_040, Some(1)).matches(&g));
        assert!(!p(1_000_000 + 16 * 60, Some(103)).matches(&g));
        assert!(!p(1_000_000 - 4 * 60, Some(103)).matches(&g));
        let aram = GameRef {
            game_type: "ARAM".into(),
            game_map: "HOWLING_ABYSS".into(),
            ..g.clone()
        };
        assert!(!p(1_000_040, None).matches(&aram));
    }

    #[test]
    fn eog_dumps_keep_last_10() {
        let dir = tmp_dir("eog");
        for id in 0..13 {
            save_eog_dump(&dir, id, &serde_json::json!({"gameId": id}));
            // Distinct mtimes so "newest" is well defined.
            std::thread::sleep(Duration::from_millis(15));
        }
        let mut names: Vec<String> = std::fs::read_dir(dir.join("debug"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), MAX_EOG_DUMPS);
        assert!(!names.contains(&"eog-0.json".to_string()));
        assert!(names.contains(&"eog-12.json".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live OP.GG check against a public pro account: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_sync_public_account() {
        let dir = tmp_dir("live");
        let g = Games::new(reqwest::Client::new(), &dir);
        let a = Account {
            game_name: "Hide on bush".into(),
            tag_line: "KR1".into(),
            region: "KR".into(),
        };
        let added = tauri::async_runtime::block_on(g.sync(&a, || {})).unwrap();
        let list = g.store.summaries();
        eprintln!("added {added}, stored {}", list.len());
        assert!(added > 0 && added == list.len());
        let rec = g.store.get(&list[0].id).unwrap();
        assert_eq!(rec.players.len(), 10);
        assert!(rec.me.is_some());
        // A second sync finds nothing new.
        assert_eq!(
            tauri::async_runtime::block_on(g.sync(&a, || {})).unwrap(),
            0
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
