//! HTTP client for LHM's built-in web server (SPEC §3.2, §3.5).
//!
//! One request per poll: `GET {base}/data.json`, gzip enabled per config,
//! optional HTTP Basic auth, then sanitize + parse into a [`Snapshot`]
//! (SPEC Q1/Q2 in [`crate::lhm::raw`]).

use std::time::{Duration, Instant};

use crate::config::ServerConfig;
use crate::lhm::model::Snapshot;

/// Typed failures; the variants drive the UI failure matrix (SPEC §10.1)
/// and the `--probe` exit codes (3 = connect, 4 = auth, SPEC §9).
#[derive(Debug, thiserror::Error)]
pub enum LhmError {
    #[error("cannot reach LibreHardwareMonitor at {url}: {source}")]
    Connect { url: String, source: reqwest::Error },
    #[error("authentication failed for {url} — check server.username/password")]
    Auth { url: String },
    #[error("unexpected response from {url}: HTTP {status}")]
    Http { url: String, status: u16 },
    #[error("invalid payload from {url}: {message}")]
    BadPayload { url: String, message: String },
}

/// One successful poll.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub snapshot: Snapshot,
    pub latency: Duration,
    pub bytes: usize,
}

pub struct LhmClient {
    http: reqwest::Client,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
}

impl LhmClient {
    pub fn new(server: &ServerConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(server.request_timeout_ms))
            .gzip(server.gzip)
            .user_agent(concat!("lhm-tui/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base_url: server.url.trim_end_matches('/').to_string(),
            username: server.username.clone().filter(|user| !user.is_empty()),
            password: resolve_password(server),
        })
    }

    /// Base URL without a trailing slash.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `GET /ResetAllMinMax` (SPEC §3.2; the `m` key).
    pub async fn reset_min_max(&self) -> Result<(), LhmError> {
        let url = format!("{}/ResetAllMinMax", self.base_url);
        let mut request = self.http.get(&url);
        if let Some(username) = &self.username {
            request = request.basic_auth(username, self.password.as_deref());
        }
        let response = request.send().await.map_err(|source| LhmError::Connect {
            url: url.clone(),
            source,
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(LhmError::Auth { url });
        }
        if !status.is_success() {
            return Err(LhmError::Http {
                url,
                status: status.as_u16(),
            });
        }
        Ok(())
    }

    /// Set a control sensor's software duty (`POST /Sensor?action=Set`,
    /// SPEC §3.2). `None` hands the fan back to automatic/driver control
    /// (`value=null` → `SetDefault`).
    ///
    /// POST (empty JSON body) rather than GET: LHM's GET path lets handler
    /// exceptions escape the listener (the request hangs with no reply, and
    /// success carries no `result` marker), while POST always answers
    /// `{"result":"ok"}` or `{"result":"fail","message":…}`.
    pub async fn set_sensor_control(
        &self,
        sensor_id: &str,
        value: Option<f32>,
    ) -> Result<(), LhmError> {
        let url = format!("{}/Sensor", self.base_url);
        let value_text = match value {
            Some(value) => format!("{value:.1}"),
            None => "null".to_string(),
        };
        let mut request = self
            .http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .query(&[
                ("action", "Set"),
                ("id", sensor_id),
                ("value", value_text.as_str()),
            ])
            // LHM's HttpListener rejects bodyless POSTs with 411 unless an
            // explicit zero length is present; the parameters live in the
            // query, so the body stays empty.
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .body(String::new());
        if let Some(username) = &self.username {
            request = request.basic_auth(username, self.password.as_deref());
        }
        let response = request.send().await.map_err(|source| LhmError::Connect {
            url: url.clone(),
            source,
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(LhmError::Auth { url });
        }
        if !status.is_success() {
            return Err(LhmError::Http {
                url,
                status: status.as_u16(),
            });
        }
        let body = response.text().await.map_err(|source| LhmError::Connect {
            url: url.clone(),
            source,
        })?;
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| LhmError::BadPayload {
                url: url.clone(),
                message: err.to_string(),
            })?;
        // POST answers `{"result":"ok"}`; LHM's older GET path answered `{}`
        // for a successful set, so anything without an explicit `fail` counts
        // as accepted.
        if parsed.get("result").and_then(|result| result.as_str()) != Some("fail") {
            return Ok(());
        }
        let message = parsed
            .get("message")
            .and_then(|message| message.as_str())
            .unwrap_or("sensor write rejected")
            .to_string();
        Err(LhmError::BadPayload { url, message })
    }

    /// Fetch and normalize `/data.json` (one poll).
    pub async fn fetch_data_json(&self) -> Result<Fetched, LhmError> {
        let url = format!("{}/data.json", self.base_url);
        let started = Instant::now();

        let mut request = self.http.get(&url);
        if let Some(username) = &self.username {
            request = request.basic_auth(username, self.password.as_deref());
        }

        let response = request.send().await.map_err(|source| LhmError::Connect {
            url: url.clone(),
            source,
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(LhmError::Auth { url });
        }
        if !status.is_success() {
            return Err(LhmError::Http {
                url,
                status: status.as_u16(),
            });
        }

        let body = response.text().await.map_err(|source| LhmError::Connect {
            url: url.clone(),
            source,
        })?;
        let latency = started.elapsed();
        let bytes = body.len();

        let snapshot =
            crate::lhm::parse::parse_payload(&body).map_err(|err| LhmError::BadPayload {
                url,
                message: err.to_string(),
            })?;

        Ok(Fetched {
            snapshot,
            latency,
            bytes,
        })
    }
}

/// Password resolution order (SPEC §8.3): inline password (CLI/env already
/// merged) → `password_env` variable → first line of `password_file`.
fn resolve_password(server: &ServerConfig) -> Option<String> {
    if let Some(password) = &server.password {
        return Some(password.clone());
    }
    if let Some(name) = &server.password_env {
        if let Ok(password) = std::env::var(name) {
            return Some(password);
        }
    }
    if let Some(path) = &server.password_file {
        if let Ok(text) = std::fs::read_to_string(path) {
            return Some(text.lines().next().unwrap_or_default().trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_resolution_order() {
        let mut server = ServerConfig::default();
        assert_eq!(resolve_password(&server), None);

        server.password_file = Some("does-not-exist.txt".to_string());
        assert_eq!(resolve_password(&server), None, "missing file → None");

        server.password_env = Some("LHM_TUI_TEST_UNSET_VAR".to_string());
        server.password_file = None;
        assert_eq!(resolve_password(&server), None, "unset env var → None");

        server.password = Some("inline".to_string());
        assert_eq!(resolve_password(&server).as_deref(), Some("inline"));
    }
}
