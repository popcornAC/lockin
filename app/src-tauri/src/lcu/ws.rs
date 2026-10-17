//! LCU WebSocket (WAMP 1.0). We subscribe to a few specific `OnJsonApiEvent_*`
//! topics rather than the firehose `OnJsonApiEvent`, to keep traffic tiny.

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, http::HeaderValue};

use super::client::{LcuClient, tls_connector};

/// WAMP opcodes we use.
const WAMP_SUBSCRIBE: u8 = 5;
const WAMP_EVENT: u8 = 8;

/// Topics we listen to.
pub const TOPICS: [&str; 3] = [
    "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",
    "OnJsonApiEvent_lol-matchmaking_v1_ready-check",
    "OnJsonApiEvent_lol-champ-select_v1_session",
];

/// One JSON API event.
#[derive(Debug, Clone, PartialEq)]
pub struct LcuEvent {
    pub uri: String,
    /// "Create", "Update" or "Delete".
    pub event_type: String,
    pub data: Value,
}

/// Parse a WAMP text frame: `[8, "<topic>", {"uri":..,"eventType":..,"data":..}]`.
pub fn parse_wamp_event(text: &str) -> Option<LcuEvent> {
    let v: Value = serde_json::from_str(text).ok()?;
    let arr = v.as_array()?;
    if arr.len() < 3 || arr[0].as_u64()? != WAMP_EVENT as u64 {
        return None;
    }
    let payload = &arr[2];
    Some(LcuEvent {
        uri: payload.get("uri")?.as_str()?.to_string(),
        event_type: payload
            .get("eventType")
            .and_then(|e| e.as_str())
            .unwrap_or("Update")
            .to_string(),
        data: payload.get("data").cloned().unwrap_or(Value::Null),
    })
}

pub fn subscribe_message(topic: &str) -> String {
    serde_json::json!([WAMP_SUBSCRIBE, topic]).to_string()
}

/// Connect and subscribe. Returns a receiver of events; the channel closes when
/// the socket closes (client exited or restarted).
pub async fn connect(lcu: &LcuClient) -> Result<mpsc::Receiver<LcuEvent>, String> {
    let url = format!("wss://127.0.0.1:{}/", lcu.creds.port);
    let mut req = url.into_client_request().map_err(|e| e.to_string())?;
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&LcuClient::basic_auth(&lcu.creds.token))
            .map_err(|e| e.to_string())?,
    );
    let connector = tokio_tungstenite::Connector::NativeTls(tls_connector(lcu.tls_mode));
    let (ws, _) =
        tokio_tungstenite::connect_async_tls_with_config(req, None, false, Some(connector))
            .await
            .map_err(|e| e.to_string())?;
    let (mut sink, mut stream) = ws.split();
    for t in TOPICS {
        sink.send(tungstenite::Message::Text(subscribe_message(t).into()))
            .await
            .map_err(|e| e.to_string())?;
    }

    let (tx, rx) = mpsc::channel(64);
    tauri::async_runtime::spawn(async move {
        while let Some(msg) = stream.next().await {
            let text = match msg {
                Ok(tungstenite::Message::Text(t)) => t,
                Ok(tungstenite::Message::Ping(p)) => {
                    let _ = sink.send(tungstenite::Message::Pong(p)).await;
                    continue;
                }
                Ok(tungstenite::Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            if let Some(ev) = parse_wamp_event(&text)
                && tx.send(ev).await.is_err()
            {
                break;
            }
        }
        // Dropping tx closes the channel => session loop notices the disconnect.
    });
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event() {
        let t = r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"data":"ChampSelect","eventType":"Update","uri":"/lol-gameflow/v1/gameflow-phase"}]"#;
        let e = parse_wamp_event(t).unwrap();
        assert_eq!(e.uri, "/lol-gameflow/v1/gameflow-phase");
        assert_eq!(e.event_type, "Update");
        assert_eq!(e.data, Value::String("ChampSelect".into()));
    }

    #[test]
    fn ignores_non_events() {
        assert!(parse_wamp_event(r#"[0,"session","1","server"]"#).is_none());
        assert!(parse_wamp_event("not json").is_none());
        assert!(parse_wamp_event(r#"[8,"topic"]"#).is_none());
        assert!(parse_wamp_event(r#"[8,"topic",{"data":1}]"#).is_none());
    }

    #[test]
    fn subscribe_frame() {
        assert_eq!(
            subscribe_message("OnJsonApiEvent_x"),
            r#"[5,"OnJsonApiEvent_x"]"#
        );
    }
}
