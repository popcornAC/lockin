//! OP.GG's public MCP server (no key). JSON-RPC `tools/call` over HTTP; the tool
//! result is a compact text format parsed by [`textfmt`].

pub mod builds;
pub mod games;
pub mod matches;
pub mod profile;
pub mod textfmt;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

const ENDPOINT: &str = "https://mcp-api.op.gg/mcp";
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Call one MCP tool and return its parsed result tree.
pub async fn call_tool(
    http: &reqwest::Client,
    tool: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments },
    });
    let resp = http
        .post(ENDPOINT)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .timeout(timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OP.GG request failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("OP.GG read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("OP.GG returned HTTP {status}"));
    }
    let rpc = parse_rpc_body(&text, id)?;
    let content = extract_text(&rpc)?;
    textfmt::parse(&content).map_err(|e| format!("OP.GG response format not understood: {e}"))
}

/// The body is either plain JSON or an SSE stream (`data: {...}` lines).
pub fn parse_rpc_body(body: &str, id: u64) -> Result<Value, String> {
    let t = body.trim_start();
    if t.starts_with('{') {
        return serde_json::from_str(t).map_err(|e| format!("bad JSON-RPC body: {e}"));
    }
    // SSE: each event's data lines joined; take the response with our id.
    let mut events = Vec::new();
    let mut cur = String::new();
    for line in body.lines() {
        if let Some(d) = line.strip_prefix("data:") {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(d.strip_prefix(' ').unwrap_or(d));
        } else if line.trim().is_empty() && !cur.is_empty() {
            events.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        events.push(cur);
    }
    events
        .iter()
        .filter_map(|e| serde_json::from_str::<Value>(e).ok())
        .find(|v| {
            v["id"].as_u64() == Some(id) && (v.get("result").is_some() || v.get("error").is_some())
        })
        .ok_or_else(|| "no JSON-RPC response in event stream".to_string())
}

/// `result.content[0].text`, or the JSON-RPC / tool error message.
pub fn extract_text(rpc: &Value) -> Result<String, String> {
    if let Some(err) = rpc.get("error") {
        return Err(err["message"]
            .as_str()
            .unwrap_or("unknown OP.GG error")
            .to_string());
    }
    let result = rpc.get("result").ok_or("JSON-RPC response has no result")?;
    let text = result["content"]
        .as_array()
        .and_then(|c| c.iter().find(|i| i["type"] == "text"))
        .and_then(|i| i["text"].as_str())
        .ok_or("tool result has no text content")?;
    if result["isError"].as_bool() == Some(true) {
        return Err(text.chars().take(200).collect());
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_json_body() {
        let body = include_str!("../../fixtures/opgg-ahri-mid.rpc.json");
        let rpc = parse_rpc_body(body, 2).unwrap();
        let text = extract_text(&rpc).unwrap();
        assert!(text.starts_with("class LolGetChampionAnalysis"));
        assert!(textfmt::parse(&text).is_ok());
    }

    #[test]
    fn sse_body() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"class A: x\\nA(1)\"}]}}\n\n";
        let rpc = parse_rpc_body(body, 7).unwrap();
        assert_eq!(
            textfmt::parse(&extract_text(&rpc).unwrap()).unwrap()["x"],
            1
        );
        assert!(parse_rpc_body(body, 8).is_err());
        assert!(parse_rpc_body("garbage", 1).is_err());
    }

    #[test]
    fn errors() {
        let e = json!({"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Unknown champion provided."}});
        assert_eq!(extract_text(&e).unwrap_err(), "Unknown champion provided.");
        let e = json!({"result":{"isError":true,"content":[{"type":"text","text":"boom"}]}});
        assert_eq!(extract_text(&e).unwrap_err(), "boom");
        assert!(extract_text(&json!({"result":{}})).is_err());
    }
}
