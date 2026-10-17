//! User toggles, persisted with `tauri-plugin-store` (`settings.json` in the
//! app data dir).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const KEY: &str = "settings";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Auto-accept the ready check. Off by default.
    pub auto_accept: bool,
    /// Apply the build automatically when you lock in.
    pub auto_import: bool,
    pub import_runes: bool,
    pub import_spells: bool,
    pub import_items: bool,
    /// Use OP.GG builds (falls back to the client's recommendations).
    pub use_opgg: bool,
    /// OP.GG rank filter, e.g. "gold_plus".
    pub opgg_tier: String,
    /// Look up visible players' rank/winrate via OP.GG.
    pub player_lookup: bool,
    /// Mark parties (needs `player_lookup`): your own from the client, others
    /// inferred in game from recent OP.GG match history.
    pub detect_premades: bool,
    /// Start (hidden in the tray) when you log in to the OS.
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_accept: false,
            auto_import: true,
            import_runes: true,
            import_spells: true,
            import_items: true,
            use_opgg: true,
            opgg_tier: crate::opgg::builds::DEFAULT_TIER.to_string(),
            player_lookup: true,
            detect_premades: true,
            launch_at_login: false,
        }
    }
}

impl Settings {
    pub fn from_value(v: Option<serde_json::Value>) -> Self {
        let mut s: Settings = v
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        s.sanitize();
        s
    }

    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        Self::from_value(app.store(STORE_FILE).ok().and_then(|s| s.get(KEY)))
    }

    pub fn sanitize(&mut self) {
        if !crate::opgg::builds::valid_tier(&self.opgg_tier) {
            self.opgg_tier = crate::opgg::builds::DEFAULT_TIER.to_string();
        }
    }

    pub fn save<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
        store.set(KEY, serde_json::to_value(self).map_err(|e| e.to_string())?);
        store.save().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults() {
        let d = Settings::default();
        assert!(!d.auto_accept, "auto-accept must be off by default");
        assert!(!d.launch_at_login);
        assert_eq!(d.opgg_tier, "gold_plus");
        assert!(d.player_lookup && d.detect_premades);
        assert_eq!(Settings::from_value(None), d);
    }

    #[test]
    fn partial_and_bad_values() {
        let s = Settings::from_value(Some(json!({"auto_accept": true})));
        assert!(s.auto_accept && s.import_runes);
        let s = Settings::from_value(Some(json!({"opgg_tier": "bogus"})));
        assert_eq!(s.opgg_tier, "gold_plus");
        assert_eq!(
            Settings::from_value(Some(json!("garbage"))),
            Settings::default()
        );
    }
}
