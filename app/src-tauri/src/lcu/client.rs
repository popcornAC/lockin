//! HTTPS client for the LCU (and the Live Client Data API), scoped to 127.0.0.1.
//!
//! TLS: the client serves a certificate issued by Riot's own root CA
//! ("LoL Game Engineering Certificate Authority"). We first try to pin that root
//! (only that CA is trusted, hostname not checked since the cert isn't issued for
//! an IP SAN we can rely on). If the OS TLS stack refuses that (it is an old v1,
//! SHA-1 signed root, which some stacks reject), we fall back to accepting the
//! certificate unverified. Either way the client can only ever talk to
//! `https://127.0.0.1:<port>`: URLs are built here from a path, redirects are
//! disabled and proxies are bypassed.

use std::time::Duration;

use base64::Engine;
use reqwest::{Method, StatusCode};
use serde_json::Value;

use super::discovery::Credentials;

const RIOT_ROOT_PEM: &[u8] = include_bytes!("riotgames.pem");

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    /// Certificate verified against Riot's pinned root CA.
    PinnedRiotRoot,
    /// Certificate not verified (localhost only).
    UnverifiedLocalhost,
}

pub fn tls_connector(mode: TlsMode) -> native_tls::TlsConnector {
    let mut b = native_tls::TlsConnector::builder();
    match mode {
        TlsMode::PinnedRiotRoot => {
            if let Ok(cert) = native_tls::Certificate::from_pem(RIOT_ROOT_PEM) {
                b.add_root_certificate(cert);
            }
            b.disable_built_in_roots(true);
            b.danger_accept_invalid_hostnames(true);
        }
        TlsMode::UnverifiedLocalhost => {
            b.danger_accept_invalid_certs(true);
        }
    }
    // Building only fails on broken OS TLS setups; fall back to defaults then.
    b.build().unwrap_or_else(|_| {
        native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("native TLS unavailable")
    })
}

fn http_client(mode: TlsMode, timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls_connector(mode))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("failed to build HTTP client")
}

#[derive(Debug)]
pub enum LcuError {
    /// Couldn't reach the client (not running, starting up, TLS failure...).
    Transport(String),
    /// The client answered with a non-success status.
    Status(StatusCode, String),
    /// Response body wasn't what we expected.
    Decode(String),
}

impl std::fmt::Display for LcuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LcuError::Transport(e) => write!(f, "could not reach client: {e}"),
            LcuError::Status(s, body) => {
                let msg = serde_json::from_str::<Value>(body)
                    .ok()
                    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                    .unwrap_or_else(|| body.chars().take(200).collect());
                write!(f, "client returned {s}: {msg}")
            }
            LcuError::Decode(e) => write!(f, "unexpected response: {e}"),
        }
    }
}

pub type LcuResult<T> = Result<T, LcuError>;

/// A connected LCU instance.
pub struct LcuClient {
    pub creds: Credentials,
    pub tls_mode: TlsMode,
    http: reqwest::Client,
    auth: String,
}

impl LcuClient {
    pub fn base_url(port: u16) -> String {
        format!("https://127.0.0.1:{port}")
    }

    pub fn basic_auth(token: &str) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(format!("riot:{token}"));
        format!("Basic {b64}")
    }

    /// Probe the client, trying the pinned root first. Errors while the client
    /// isn't answering yet.
    pub async fn connect(creds: Credentials) -> Result<Self, String> {
        let mut last_err = String::new();
        for mode in [TlsMode::PinnedRiotRoot, TlsMode::UnverifiedLocalhost] {
            let c = LcuClient {
                http: http_client(mode, Duration::from_secs(8)),
                auth: Self::basic_auth(&creds.token),
                creds: creds.clone(),
                tls_mode: mode,
            };
            match c
                .request(Method::GET, "/lol-gameflow/v1/gameflow-phase", None)
                .await
            {
                // Any HTTP answer (even 404 while plugins load) proves TLS + port are right.
                Ok(_) | Err(LcuError::Status(..)) => return Ok(c),
                Err(e) => last_err = e.to_string(),
            }
        }
        Err(last_err)
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> LcuResult<Value> {
        assert!(path.starts_with('/'), "LCU paths must be absolute");
        let url = format!("{}{}", Self::base_url(self.creds.port), path);
        let mut req = self
            .http
            .request(method, url)
            .header(reqwest::header::AUTHORIZATION, &self.auth)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| LcuError::Transport(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LcuError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(LcuError::Status(status, text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| LcuError::Decode(e.to_string()))
    }

    pub async fn get(&self, path: &str) -> LcuResult<Value> {
        self.request(Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> LcuResult<Value> {
        self.request(Method::POST, path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: &Value) -> LcuResult<Value> {
        self.request(Method::PUT, path, Some(body)).await
    }

    pub async fn patch(&self, path: &str, body: &Value) -> LcuResult<Value> {
        self.request(Method::PATCH, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> LcuResult<Value> {
        self.request(Method::DELETE, path, None).await
    }
}

/// Client for the in-game Live Client Data API (`https://127.0.0.1:2999`, no auth,
/// same Riot-issued certificate). Only exists while a game is loading/running.
pub struct LiveClient {
    http: reqwest::Client,
}

impl LiveClient {
    pub fn new(mode: TlsMode) -> Self {
        Self {
            http: http_client(mode, Duration::from_secs(3)),
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value, String> {
        assert!(path.starts_with('/'));
        let resp = self
            .http
            .get(format!("https://127.0.0.1:2999{path}"))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("status {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_header() {
        // base64("riot:abc") = cmlvdDphYmM=
        assert_eq!(LcuClient::basic_auth("abc"), "Basic cmlvdDphYmM=");
    }

    #[test]
    fn riot_root_parses() {
        assert!(native_tls::Certificate::from_pem(RIOT_ROOT_PEM).is_ok());
    }

    #[test]
    fn error_message_extracts_lcu_message() {
        let e = LcuError::Status(
            StatusCode::BAD_REQUEST,
            r#"{"errorCode":"RPC_ERROR","httpStatus":400,"message":"Max pages reached"}"#.into(),
        );
        assert!(e.to_string().contains("Max pages reached"));
    }
}
