//! The background service: finds the client, keeps the WebSocket open, reacts
//! to gameflow / ready-check / champ-select events, and publishes a `Status`
//! snapshot to the window.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;

use crate::assets::{self, GameAssets};
use crate::import::client_recs::{ClientRecommendations, default_position};
use crate::import::source::{BuildContext, BuildQuery, BuildSource, position_label};
use crate::import::{self, ImportReport, Parts};
use crate::lcu::api::{self, PartyMember, Selection};
use crate::lcu::discovery::Discovery;
use crate::lcu::ws::{self, LcuEvent};
use crate::lcu::{LcuClient, LiveClient, TlsMode};
use crate::opgg::builds::OpggSource;
use crate::opgg::profile::opgg_region;
use crate::players::{self, Lookup, PlayerView};
use crate::premades;
use crate::settings::Settings;

const IDLE_POLL: Duration = Duration::from_secs(4);
const MAX_PREFETCH: usize = 4;
const LOG_LINES: usize = 40;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ChampSelectView {
    pub champion_id: i64,
    pub champion_name: String,
    pub position: String,
    pub locked: bool,
    pub map_id: Option<i64>,
    pub queue_id: Option<i64>,
    pub supported: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    /// "searching" | "connected"
    pub connection: String,
    pub tls: Option<TlsMode>,
    pub creds_source: Option<String>,
    pub phase: String,
    pub patch: Option<String>,
    pub region: Option<String>,
    pub champ_select: Option<ChampSelectView>,
    pub last_import: Option<ImportReport>,
    pub importing: bool,
    /// "" | "champ_select" | "in_game"
    pub players_context: String,
    pub players: Vec<PlayerView>,
    pub log: VecDeque<String>,
}

#[derive(Default)]
struct ChampSelectTracker {
    game_id: i64,
    map_queue: Option<(Option<i64>, Option<i64>)>,
    last_applied: Option<(i64, String)>,
    prefetched: HashSet<i64>,
    looked_up: HashSet<String>,
}

/// One player to look up.
struct Row {
    riot_id: (String, String),
    is_self: bool,
    champion: Option<String>,
    team: Option<String>,
}

/// What party detection may do for a lookup. Champ select only ever gets
/// `KnownOnly`: anonymous allies are never inferred about.
#[derive(Clone, Copy, PartialEq)]
enum Parties {
    KnownOnly,
    KnownAndInferred,
}

pub struct Service {
    app: AppHandle,
    pub settings: Mutex<Settings>,
    status: Mutex<Status>,
    lcu: Mutex<Option<Arc<LcuClient>>>,
    pub opgg: Arc<OpggSource>,
    client_recs: Arc<ClientRecommendations>,
    http: reqwest::Client,
    champions: Mutex<HashMap<i64, (String, String)>>,
    cs: Mutex<ChampSelectTracker>,
    lookup: Lookup,
    /// Your own lobby, captured before champ select (from the client).
    party: Mutex<Vec<PartyMember>>,
    /// Lower-case Riot IDs of your own party (lobby + matched champ select cells).
    party_ids: Mutex<HashSet<String>>,
    /// Likely premade pairs by lower-case Riot ID, for the current game only.
    inferred: Mutex<Vec<(String, String, u32)>>,
    last_accept: Mutex<Option<std::time::Instant>>,
    /// Bumped on every phase change; long-running per-phase tasks stop when it changes.
    phase_gen: AtomicU64,
    data_dir: PathBuf,
    /// Icon lookup tables for the window (see [`assets`]).
    assets: Mutex<Option<GameAssets>>,
}

fn now_hms() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // UTC time of day; good enough for a short log.
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

impl Service {
    pub fn new(app: AppHandle, data_dir: PathBuf) -> Arc<Self> {
        let mut settings = Settings::load(&app);
        // The OS login item is the source of truth for launch-at-login.
        if let Ok(on) = app.autolaunch().is_enabled() {
            settings.launch_at_login = on;
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("lockin/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("http client");
        let opgg = Arc::new(OpggSource::new(
            http.clone(),
            data_dir.clone(),
            settings.opgg_tier.clone(),
        ));
        Arc::new(Self {
            app,
            settings: Mutex::new(settings),
            status: Mutex::new(Status {
                connection: "searching".into(),
                phase: "None".into(),
                ..Default::default()
            }),
            lcu: Mutex::new(None),
            opgg,
            client_recs: Arc::new(ClientRecommendations::new(http.clone())),
            http,
            champions: Mutex::new(HashMap::new()),
            cs: Mutex::new(ChampSelectTracker::default()),
            lookup: Lookup::default(),
            party: Mutex::new(Vec::new()),
            party_ids: Mutex::new(HashSet::new()),
            inferred: Mutex::new(Vec::new()),
            last_accept: Mutex::new(None),
            phase_gen: AtomicU64::new(0),
            data_dir,
            assets: Mutex::new(None),
        })
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn assets(&self) -> Option<GameAssets> {
        self.assets.lock().unwrap().clone()
    }

    fn set_assets(&self, a: GameAssets) {
        *self.assets.lock().unwrap() = Some(a.clone());
        let _ = self.app.emit("assets", a);
    }

    fn update(&self, f: impl FnOnce(&mut Status)) {
        let snapshot = {
            let mut s = self.status.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        let _ = self.app.emit("status", snapshot);
    }

    pub fn log(&self, msg: impl Into<String>) {
        let line = format!("{} {}", now_hms(), msg.into());
        eprintln!("[lockin] {line}");
        self.update(|s| {
            s.log.push_front(line);
            s.log.truncate(LOG_LINES);
        });
    }

    pub fn set_settings(&self, mut new: Settings) -> Result<Settings, String> {
        new.sanitize();
        let al = self.app.autolaunch();
        if al.is_enabled().ok() != Some(new.launch_at_login) {
            let r = if new.launch_at_login {
                al.enable()
            } else {
                al.disable()
            };
            if let Err(e) = r {
                self.log(format!("Could not change launch-at-login: {e}"));
                new.launch_at_login = al.is_enabled().unwrap_or(false);
            }
        }
        new.save(&self.app)
            .map_err(|e| format!("could not save settings: {e}"))?;
        *self.opgg.tier.lock().unwrap() = new.opgg_tier.clone();
        *self.settings.lock().unwrap() = new.clone();
        let _ = self.app.emit("settings", &new);
        self.retag();
        Ok(new)
    }

    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn sources(&self) -> Vec<Arc<dyn BuildSource>> {
        let mut v: Vec<Arc<dyn BuildSource>> = Vec::new();
        if self.settings().use_opgg {
            v.push(self.opgg.clone());
        }
        v.push(self.client_recs.clone());
        v
    }

    pub fn current_lcu(&self) -> Option<Arc<LcuClient>> {
        self.lcu.lock().unwrap().clone()
    }

    // ---------- main loop ----------

    pub async fn run(self: Arc<Self>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.refresh_patch().await });

        let mut disc = Discovery::default();
        loop {
            let (d, creds) = tauri::async_runtime::spawn_blocking(move || {
                let c = disc.find();
                (disc, c)
            })
            .await
            .unwrap_or_else(|_| (Discovery::default(), None));
            disc = d;

            let Some(creds) = creds else {
                tokio::time::sleep(IDLE_POLL).await;
                continue;
            };
            let source = creds.source;
            let client = match LcuClient::connect(creds).await {
                Ok(c) => Arc::new(c),
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            let mut rx = match ws::connect(&client).await {
                Ok(rx) => rx,
                Err(e) => {
                    self.log(format!("WebSocket not ready yet ({e}), retrying"));
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };
            *self.lcu.lock().unwrap() = Some(client.clone());
            self.update(|s| {
                s.connection = "connected".into();
                s.tls = Some(client.tls_mode);
                s.creds_source = Some(source.to_string());
            });
            self.log(format!(
                "Connected to League client on port {} ({})",
                client.creds.port,
                match client.tls_mode {
                    TlsMode::PinnedRiotRoot => "certificate verified against Riot root",
                    TlsMode::UnverifiedLocalhost => "certificate not verified, localhost only",
                }
            ));
            self.clone().on_connected(client.clone()).await;

            while let Some(ev) = rx.recv().await {
                self.clone().handle_event(&client, ev).await;
            }
            self.on_disconnected();
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    fn on_disconnected(&self) {
        *self.lcu.lock().unwrap() = None;
        self.champions.lock().unwrap().clear();
        self.party.lock().unwrap().clear();
        self.party_ids.lock().unwrap().clear();
        self.inferred.lock().unwrap().clear();
        self.phase_gen.fetch_add(1, Ordering::SeqCst);
        self.update(|s| {
            s.connection = "searching".into();
            s.tls = None;
            s.creds_source = None;
            s.phase = "None".into();
            s.champ_select = None;
            s.players.clear();
            s.players_context.clear();
        });
        self.log("League client closed; waiting for it to start");
    }

    async fn refresh_patch(&self) {
        let r = async {
            let v: Value = self
                .http
                .get("https://ddragon.leagueoflegends.com/api/versions.json")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            v.get(0)
                .and_then(|s| s.as_str())
                .map(String::from)
                .ok_or("no versions".to_string())
        }
        .await;
        match r {
            Ok(version) => {
                if let Some(p) = api::patch_of(&version) {
                    self.update(|s| s.patch = Some(p));
                }
                match assets::load(&self.http, &self.data_dir, &version).await {
                    Ok(a) => self.set_assets(a),
                    Err(e) => {
                        self.log(format!("Could not load icon data from Data Dragon: {e}"));
                        self.load_cached_assets();
                    }
                }
            }
            Err(e) => {
                self.log(format!(
                    "Could not read current patch from Data Dragon: {e}"
                ));
                self.load_cached_assets();
            }
        }
    }

    fn load_cached_assets(&self) {
        if let Some(a) = assets::load_cached(&self.data_dir) {
            self.set_assets(a);
        }
    }

    async fn on_connected(self: Arc<Self>, c: Arc<LcuClient>) {
        if self.status.lock().unwrap().patch.is_none()
            && let Some(p) = api::client_patch(&c).await
        {
            self.update(|s| s.patch = Some(p));
        }
        let region = api::region(&c).await.and_then(|r| opgg_region(&r));
        self.update(|s| s.region = region);
        let phase = api::gameflow_phase(&c)
            .await
            .unwrap_or_else(|_| "None".into());
        self.on_phase(&c, phase).await;
    }

    async fn handle_event(self: Arc<Self>, c: &Arc<LcuClient>, ev: LcuEvent) {
        match ev.uri.as_str() {
            "/lol-gameflow/v1/gameflow-phase" => {
                let phase = ev.data.as_str().unwrap_or("None").to_string();
                self.on_phase(c, phase).await;
            }
            "/lol-matchmaking/v1/ready-check" => {
                if ev.event_type != "Delete" {
                    self.maybe_accept(c, &ev.data).await;
                }
            }
            "/lol-champ-select/v1/session" => {
                if ev.event_type == "Delete" {
                    self.update(|s| s.champ_select = None);
                } else {
                    self.on_session(c, &ev.data).await;
                }
            }
            _ => {}
        }
    }

    async fn on_phase(self: Arc<Self>, c: &Arc<LcuClient>, phase: String) {
        let changed = self.status.lock().unwrap().phase != phase;
        if !changed {
            return;
        }
        let generation = self.phase_gen.fetch_add(1, Ordering::SeqCst) + 1;
        self.update(|s| s.phase = phase.clone());
        if matches!(
            phase.as_str(),
            "Lobby" | "Matchmaking" | "ReadyCheck" | "ChampSelect"
        ) {
            let me = self.clone();
            let c = c.clone();
            tauri::async_runtime::spawn(async move { me.refresh_party(&c).await });
        }
        match phase.as_str() {
            "ReadyCheck" => {
                if let Ok(rc) = api::ready_check(c).await {
                    self.maybe_accept(c, &rc).await;
                }
            }
            "ChampSelect" => {
                *self.cs.lock().unwrap() = ChampSelectTracker::default();
                self.inferred.lock().unwrap().clear();
                self.update(|s| {
                    s.players.clear();
                    s.players_context = "champ_select".into();
                });
                if let Ok(sess) = api::champ_select_session(c).await {
                    self.on_session(c, &sess).await;
                }
            }
            "InProgress" | "GameStart" => {
                self.update(|s| s.champ_select = None);
                if self.status.lock().unwrap().players_context != "in_game" {
                    self.inferred.lock().unwrap().clear();
                }
                if self.settings().player_lookup
                    && self.status.lock().unwrap().players_context != "in_game"
                {
                    let me = self.clone();
                    let tls = c.tls_mode;
                    tauri::async_runtime::spawn(async move {
                        me.live_game_lookup(generation, tls).await
                    });
                }
            }
            "None" | "Lobby" | "Matchmaking" => {
                if phase == "None" {
                    self.party.lock().unwrap().clear();
                    self.party_ids.lock().unwrap().clear();
                }
                self.update(|s| {
                    s.champ_select = None;
                    if s.players_context == "in_game" || phase != "Matchmaking" {
                        s.players.clear();
                        s.players_context.clear();
                    }
                });
            }
            _ => {}
        }
    }

    async fn maybe_accept(&self, c: &LcuClient, rc: &Value) {
        if self.settings().auto_accept && api::should_accept(rc) {
            {
                // The phase change and the ready-check event both arrive; accept once.
                let mut last = self.last_accept.lock().unwrap();
                if last.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
                    return;
                }
                *last = Some(std::time::Instant::now());
            }
            match api::accept_ready_check(c).await {
                Ok(_) => self.log("Ready check accepted"),
                Err(e) => self.log(format!("Could not accept ready check: {e}")),
            }
        }
    }

    // ---------- champ select ----------

    async fn champion_info(&self, c: &LcuClient, id: i64) -> Option<(String, String)> {
        if id <= 0 {
            return None;
        }
        if let Some(i) = self.champions.lock().unwrap().get(&id).cloned() {
            return Some(i);
        }
        let summary = api::champion_summary(c).await.ok()?;
        let mut map = HashMap::new();
        for ch in summary.as_array().into_iter().flatten() {
            if let (Some(i), Some(a), Some(n)) =
                (ch["id"].as_i64(), ch["alias"].as_str(), ch["name"].as_str())
            {
                map.insert(i, (a.to_string(), n.to_string()));
            }
        }
        let r = map.get(&id).cloned();
        *self.champions.lock().unwrap() = map;
        r
    }

    async fn map_queue(&self, c: &LcuClient, game_id: i64) -> (Option<i64>, Option<i64>) {
        {
            let cs = self.cs.lock().unwrap();
            if cs.game_id == game_id
                && let Some(mq) = cs.map_queue
            {
                return mq;
            }
        }
        let mq = api::gameflow_map_queue(c).await.unwrap_or((None, None));
        let mut cs = self.cs.lock().unwrap();
        cs.game_id = game_id;
        cs.map_queue = Some(mq);
        mq
    }

    async fn on_session(self: Arc<Self>, c: &Arc<LcuClient>, session: &Value) {
        let Some(sel) = api::parse_selection(session) else {
            return;
        };
        let (map_id, queue_id) = self.map_queue(c, sel.game_id).await;
        let champ = sel.current_champion();
        let name = self
            .champion_info(c, champ)
            .await
            .map(|i| i.1)
            .unwrap_or_default();
        let supported = map_id.unwrap_or(11) == 11;
        self.update(|s| {
            s.champ_select = Some(ChampSelectView {
                champion_id: champ,
                champion_name: name,
                position: sel.position.clone(),
                locked: sel.is_locked(),
                map_id,
                queue_id,
                supported,
                message: (!supported)
                    .then(|| "Not supported in this mode (Summoner's Rift only)".to_string()),
            })
        });
        if !supported {
            return;
        }
        let settings = self.settings();

        if settings.player_lookup {
            let party_cells = api::party_cells(session, &self.party.lock().unwrap());
            self.clone()
                .champ_select_lookup(c.clone(), &sel, party_cells);
        }

        if sel.is_locked() {
            let key = (sel.locked_champion, sel.position.clone());
            let fire = {
                let mut cs = self.cs.lock().unwrap();
                if settings.auto_import && cs.last_applied.as_ref() != Some(&key) {
                    cs.last_applied = Some(key);
                    true
                } else {
                    false
                }
            };
            if fire {
                let me = self.clone();
                let c = c.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = me
                        .import_for(&c, &sel, map_id.unwrap_or(11), queue_id)
                        .await;
                });
            }
        } else if sel.hover_champion > 0 && settings.use_opgg && settings.auto_import {
            // Prefetch the hovered champion so locking in applies instantly.
            let go = {
                let mut cs = self.cs.lock().unwrap();
                cs.prefetched.len() < MAX_PREFETCH && cs.prefetched.insert(sel.hover_champion)
            };
            if go {
                let me = self.clone();
                let c = c.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(q) = me
                        .query_for(&c, sel.hover_champion, &sel.position, 11, queue_id)
                        .await
                    {
                        let ctx = BuildContext { query: &q, lcu: &c };
                        let _ = me.opgg.fetch(&ctx).await;
                    }
                });
            }
        }
    }

    async fn query_for(
        &self,
        c: &LcuClient,
        champ: i64,
        position: &str,
        map_id: i64,
        queue_id: Option<i64>,
    ) -> Option<BuildQuery> {
        let (alias, name) = self.champion_info(c, champ).await?;
        let fallback_position = if position.is_empty() {
            match self.client_recs.recommendations(c).await {
                Ok(r) => default_position(&r, champ).unwrap_or_default(),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };
        Some(BuildQuery {
            champion_id: champ,
            champion_alias: alias,
            champion_name: name,
            position: position.to_string(),
            fallback_position,
            map_id,
            queue_id,
            patch: self.status.lock().unwrap().patch.clone(),
        })
    }

    async fn import_for(
        &self,
        c: &LcuClient,
        sel: &Selection,
        map_id: i64,
        queue_id: Option<i64>,
    ) -> Result<ImportReport, String> {
        if map_id != 11 {
            return Err("Not supported in this mode (Summoner's Rift only)".into());
        }
        let champ = sel.current_champion();
        if champ <= 0 {
            return Err("No champion selected yet".into());
        }
        let q = self
            .query_for(c, champ, &sel.position, map_id, queue_id)
            .await
            .ok_or("Champion data not available from the client yet")?;
        let st = self.settings();
        let want = Parts {
            runes: st.import_runes,
            spells: st.import_spells,
            items: st.import_items,
        };
        self.update(|s| s.importing = true);
        self.log(format!(
            "Importing build for {} ({})",
            q.champion_name,
            position_label(&q.position_or_fallback())
        ));
        let ctx = BuildContext { query: &q, lcu: c };
        let plan = import::gather(&self.sources(), &ctx, want).await;
        let report = import::apply(c, &q, plan, want, Some(sel.spells)).await;
        for n in &report.notes {
            self.log(format!("Source skipped: {n}"));
        }
        for (part, r) in [
            ("Runes", &report.runes),
            ("Spells", &report.spells),
            ("Items", &report.items),
        ] {
            self.log(format!(
                "{part}: {:?}{} - {}",
                r.status,
                r.source
                    .as_deref()
                    .map(|s| format!(" from {s}"))
                    .unwrap_or_default(),
                r.detail
            ));
        }
        let rep = report.clone();
        self.update(|s| {
            s.importing = false;
            s.last_import = Some(rep);
        });
        Ok(report)
    }

    /// The manual "Apply" button.
    pub async fn apply_now(&self) -> Result<ImportReport, String> {
        let c = self.current_lcu().ok_or("League client not connected")?;
        let session = api::champ_select_session(&c)
            .await
            .map_err(|_| "Not in champ select".to_string())?;
        let sel = api::parse_selection(&session).ok_or("Not in champ select")?;
        let (map_id, queue_id) = self.map_queue(&c, sel.game_id).await;
        let r = self
            .import_for(&c, &sel, map_id.unwrap_or(11), queue_id)
            .await;
        if r.is_ok() {
            self.cs.lock().unwrap().last_applied =
                Some((sel.current_champion(), sel.position.clone()));
        }
        r
    }

    pub fn refresh_builds(&self) {
        let patch = self
            .status
            .lock()
            .unwrap()
            .patch
            .clone()
            .unwrap_or_else(|| "unknown".into());
        self.opgg.clear(&patch);
        self.cs.lock().unwrap().prefetched.clear();
        self.log(format!("Cleared cached OP.GG builds for patch {patch}"));
    }

    // ---------- player lookup ----------

    fn champ_select_lookup(
        self: Arc<Self>,
        c: Arc<LcuClient>,
        sel: &Selection,
        party_cells: Vec<i64>,
    ) {
        let Some(region) = self.status.lock().unwrap().region.clone() else {
            return;
        };
        // Only players the client shows by name. Anonymised allies are skipped entirely.
        let allies: Vec<_> = sel
            .allies
            .iter()
            .filter(|a| a.riot_id.is_some() || a.visible_puuid.is_some())
            .cloned()
            .collect();
        let new: Vec<_> = {
            let mut cs = self.cs.lock().unwrap();
            allies
                .into_iter()
                .filter(|a| {
                    let k = a
                        .riot_id
                        .as_ref()
                        .map(|(n, t)| players::riot_id_string(n, t))
                        .or(a.visible_puuid.clone())
                        .unwrap_or_default();
                    cs.looked_up.insert(k)
                })
                .collect()
        };
        if new.is_empty() {
            return;
        }
        tauri::async_runtime::spawn(async move {
            let mut rows = Vec::new();
            for a in new {
                let rid = match a.riot_id {
                    Some(r) => Some(r),
                    None => match &a.visible_puuid {
                        Some(p) => api::riot_id_by_puuid(&c, p).await.ok().flatten(),
                        None => None,
                    },
                };
                if let Some(r) = rid {
                    if party_cells.contains(&a.cell_id) {
                        self.party_ids
                            .lock()
                            .unwrap()
                            .insert(players::riot_id_string(&r.0, &r.1).to_lowercase());
                    }
                    rows.push(Row {
                        riot_id: r,
                        is_self: a.is_local,
                        champion: None,
                        team: None,
                    });
                }
            }
            self.run_lookup(region, rows, "champ_select", Parties::KnownOnly)
                .await;
        });
    }

    /// Your own party from the lobby. Free (local client only).
    async fn refresh_party(&self, c: &LcuClient) {
        let Ok(v) = api::lobby(c).await else {
            return;
        };
        let mut members = api::parse_lobby_members(&v);
        if members.is_empty() {
            return; // e.g. lobby already gone in champ select: keep the last one
        }
        for m in members.iter_mut().filter(|m| m.riot_id.is_none()) {
            m.riot_id = api::riot_id_by_puuid(c, &m.puuid).await.ok().flatten();
        }
        let ids: HashSet<String> = members
            .iter()
            .filter_map(|m| m.riot_id.as_ref())
            .map(|(n, t)| players::riot_id_string(n, t).to_lowercase())
            .collect();
        *self.party.lock().unwrap() = members;
        *self.party_ids.lock().unwrap() = ids;
        self.retag();
    }

    /// Recompute every player's party marker from the known party and the
    /// inferred pairs. Cheap; called whenever either changes.
    fn retag(&self) {
        let on = {
            let st = self.settings();
            st.player_lookup && st.detect_premades
        };
        let known = self.party_ids.lock().unwrap().clone();
        let inferred = self.inferred.lock().unwrap().clone();
        self.update(|s| {
            let keys: Vec<String> = s.players.iter().map(|p| p.riot_id.to_lowercase()).collect();
            let idx = |k: &str| keys.iter().position(|x| x == k);
            let known_idx: Vec<usize> = (0..keys.len())
                .filter(|&i| known.contains(&keys[i]))
                .collect();
            let pairs: Vec<_> = inferred
                .iter()
                .filter_map(|(a, b, n)| Some((idx(a)?, idx(b)?, *n)))
                .collect();
            let tags = if on {
                premades::tags(keys.len(), &known_idx, &pairs)
            } else {
                vec![None; keys.len()]
            };
            for (p, t) in s.players.iter_mut().zip(tags) {
                p.party = t;
            }
        });
    }

    async fn run_lookup(&self, region: String, rows: Vec<Row>, context: &str, parties: Parties) {
        // Show placeholders first.
        let placeholders: Vec<PlayerView> = rows
            .iter()
            .map(|r| PlayerView {
                riot_id: players::riot_id_string(&r.riot_id.0, &r.riot_id.1),
                champion: r.champion.clone(),
                team: r.team.clone(),
                is_self: r.is_self,
                profile: None,
                error: None,
                loading: true,
                party: None,
            })
            .collect();
        let ctx = context.to_string();
        self.update(|s| {
            if s.players_context != ctx {
                s.players.clear();
                s.players_context = ctx.clone();
            }
            for p in &placeholders {
                if !s.players.iter().any(|x| x.riot_id == p.riot_id) {
                    s.players.push(p.clone());
                }
            }
        });
        // Your own party is known locally: mark it straight away.
        self.retag();

        let ids: Vec<(String, String)> = rows.iter().map(|r| r.riot_id.clone()).collect();
        let profiles = async {
            let results = self.lookup.lookup_many(&self.http, &region, &ids).await;
            let found = results.iter().filter(|r| r.is_ok()).count();
            self.log(format!(
                "Looked up {} player(s) on OP.GG ({region}): {found} found",
                ids.len()
            ));
            self.update(|s| {
                for (p, r) in placeholders.iter().zip(results) {
                    if let Some(x) = s.players.iter_mut().find(|x| x.riot_id == p.riot_id) {
                        x.loading = false;
                        match r {
                            Ok(prof) => x.profile = Some(prof),
                            Err(e) => x.error = Some(e),
                        }
                    }
                }
            });
        };
        let inferred = async {
            if parties == Parties::KnownAndInferred && self.settings().detect_premades {
                self.infer_parties(&region, &rows).await;
            }
        };
        tokio::join!(profiles, inferred);
    }

    /// In game only: likely parties among players whose Riot IDs the Live
    /// Client API shows, from shared recent games on OP.GG.
    async fn infer_parties(&self, region: &str, rows: &[Row]) {
        let keys: Vec<String> = rows
            .iter()
            .map(|r| players::riot_id_string(&r.riot_id.0, &r.riot_id.1).to_lowercase())
            .collect();
        let teams: Vec<&str> = rows
            .iter()
            .map(|r| r.team.as_deref().unwrap_or(""))
            .collect();
        let known: Vec<usize> = {
            let ids = self.party_ids.lock().unwrap();
            (0..keys.len())
                .filter(|&i| ids.contains(&keys[i]))
                .collect()
        };
        let need = premades::needs_history(&teams, &known);
        let wanted: Vec<usize> = (0..rows.len()).filter(|&i| need[i]).collect();
        if wanted.is_empty() {
            return;
        }
        let ids: Vec<(String, String)> = wanted.iter().map(|&i| rows[i].riot_id.clone()).collect();
        let results = self.lookup.matches_many(&self.http, region, &ids).await;
        let ok = results.iter().filter(|r| r.is_ok()).count();
        let mut histories: Vec<Option<&[crate::opgg::matches::MatchRecord]>> =
            vec![None; rows.len()];
        for (&i, r) in wanted.iter().zip(&results) {
            histories[i] = r.as_deref().ok();
        }
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let pairs = premades::likely_pairs(&teams, &histories, now);
        let groups = premades::group(rows.len(), &pairs);
        self.log(format!(
            "Premade check: {} match list(s) from OP.GG, {ok} ok; {} likely part{}",
            ids.len(),
            groups.len(),
            if groups.len() == 1 { "y" } else { "ies" }
        ));
        *self.inferred.lock().unwrap() = pairs
            .into_iter()
            .map(|(a, b, n)| (keys[a].clone(), keys[b].clone(), n))
            .collect();
        self.retag();
    }

    async fn live_game_lookup(&self, generation: u64, tls: TlsMode) {
        let Some(region) = self.status.lock().unwrap().region.clone() else {
            return;
        };
        let mut client = LiveClient::new(tls);
        let mut tried_fallback = false;
        // The Live Client API comes up during the loading screen; poll gently.
        for _ in 0..90 {
            if self.phase_gen.load(Ordering::SeqCst) != generation {
                return;
            }
            match client.get("/liveclientdata/playerlist").await {
                Ok(v) => {
                    let list = players::parse_live_players(&v);
                    if !list.is_empty() {
                        let me = self.current_lcu();
                        let my_id = match me {
                            Some(c) => api::current_summoner(&c).await.ok().and_then(|s| s.riot_id),
                            None => None,
                        };
                        let rows = list
                            .into_iter()
                            .map(|p| {
                                let riot_id = (p.game_name, p.tag_line);
                                Row {
                                    is_self: my_id.as_ref() == Some(&riot_id),
                                    riot_id,
                                    champion: Some(p.champion),
                                    team: Some(p.team),
                                }
                            })
                            .collect();
                        self.update(|s| {
                            s.players.clear();
                            s.players_context = "in_game".into();
                        });
                        // Riot IDs are visible to everyone here, so inference is allowed.
                        self.run_lookup(region, rows, "in_game", Parties::KnownAndInferred)
                            .await;
                        return;
                    }
                }
                Err(_) if !tried_fallback && tls == TlsMode::PinnedRiotRoot => {
                    // Possibly a TLS refusal; try once without verification (localhost only).
                    tried_fallback = true;
                    client = LiveClient::new(TlsMode::UnverifiedLocalhost);
                    continue;
                }
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}
