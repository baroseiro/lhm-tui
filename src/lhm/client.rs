//! Sensor source client (SPEC §3.2, §3.5, §3.6).
//!
//! [`LhmClient`] selects between LHM's built-in web server and the direct
//! `LibreHardwareMonitorLib.dll` bridge. Both return the same [`Fetched`]
//! snapshot, so the poller, fan controls and one-shot modes are source
//! agnostic. HTTP remains the default and keeps its retry/auth behavior.

use std::time::{Duration, Instant};

use crate::config::{DataSource, ServerConfig};
use crate::lhm::dll::DllClient;
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
    #[error("LibreHardwareMonitor DLL bridge error ({path}): {message}")]
    Dll { path: String, message: String },
}

/// One successful poll.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub snapshot: Snapshot,
    pub latency: Duration,
    pub bytes: usize,
    /// DLL source only: whether the bridge has administrator privileges.
    /// `None` for HTTP (the LHM server's elevation is not observable).
    pub elevated: Option<bool>,
}

/// Source-agnostic client handed to the poller, fan-control task and
/// one-shot modes. Cheap to clone: HTTP clients share their pool and DLL
/// clients share one bridge process per library path.
#[derive(Clone)]
pub struct LhmClient {
    inner: ClientInner,
    endpoint: String,
}

#[derive(Clone)]
enum ClientInner {
    Http(HttpClient),
    Dll(DllClient),
}

impl LhmClient {
    pub fn new(server: &ServerConfig) -> anyhow::Result<Self> {
        match server.source {
            DataSource::Http => {
                let http = HttpClient::new(server)?;
                Ok(Self {
                    endpoint: http.base_url.clone(),
                    inner: ClientInner::Http(http),
                })
            }
            DataSource::Dll => {
                let dll = DllClient::connect(server).map_err(anyhow::Error::msg)?;
                Ok(Self {
                    endpoint: dll.endpoint().to_string(),
                    inner: ClientInner::Dll(dll),
                })
            }
        }
    }

    /// HTTP base URL, or `dll:<path>` in DLL mode.
    pub fn base_url(&self) -> &str {
        &self.endpoint
    }

    /// `GET /ResetAllMinMax` (SPEC §3.2; the `m` key) or the bridge
    /// equivalent.
    pub async fn reset_min_max(&self) -> Result<(), LhmError> {
        match &self.inner {
            ClientInner::Http(http) => http.reset_min_max().await,
            ClientInner::Dll(dll) => {
                let dll = dll.clone();
                let path = self.endpoint.clone();
                tokio::task::spawn_blocking(move || dll.reset_min_max())
                    .await
                    .map_err(|err| LhmError::Dll {
                        path,
                        message: format!("bridge task failed: {err}"),
                    })?
            }
        }
    }

    /// Set a control sensor's software duty (`POST /Sensor?action=Set`,
    /// SPEC §3.2). `None` hands the fan back to automatic/driver control
    /// (`value=null` → `SetDefault`).
    pub async fn set_sensor_control(
        &self,
        sensor_id: &str,
        value: Option<f32>,
    ) -> Result<(), LhmError> {
        match &self.inner {
            ClientInner::Http(http) => http.set_sensor_control(sensor_id, value).await,
            ClientInner::Dll(dll) => {
                let dll = dll.clone();
                let sensor_id = sensor_id.to_string();
                let path = self.endpoint.clone();
                tokio::task::spawn_blocking(move || dll.set_sensor_control(&sensor_id, value))
                    .await
                    .map_err(|err| LhmError::Dll {
                        path,
                        message: format!("bridge task failed: {err}"),
                    })?
            }
        }
    }

    /// Fetch and normalize one snapshot.
    pub async fn fetch_data_json(&self) -> Result<Fetched, LhmError> {
        match &self.inner {
            ClientInner::Http(http) => http.fetch_data_json().await,
            ClientInner::Dll(dll) => {
                let dll = dll.clone();
                let path = self.endpoint.clone();
                tokio::task::spawn_blocking(move || dll.fetch_snapshot())
                    .await
                    .map_err(|err| LhmError::Dll {
                        path,
                        message: format!("bridge task failed: {err}"),
                    })?
            }
        }
    }
}

#[derive(Clone)]
struct HttpClient {
    http: reqwest::Client,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
}

impl HttpClient {
    fn new(server: &ServerConfig) -> anyhow::Result<Self> {
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

    /// `GET /ResetAllMinMax` (SPEC §3.2; the `m` key).
    async fn reset_min_max(&self) -> Result<(), LhmError> {
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
    async fn set_sensor_control(
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
    async fn fetch_data_json(&self) -> Result<Fetched, LhmError> {
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
            elevated: None,
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

    #[test]
    fn endpoint_label_tracks_the_selected_source() {
        let mut server = ServerConfig::default();
        assert_eq!(server.endpoint_label(), "http://127.0.0.1:8085");

        server.source = DataSource::Dll;
        assert_eq!(server.endpoint_label(), "dll:auto");

        server.dll_path = Some(r"C:\LHM\LibreHardwareMonitorLib.dll".to_string());
        assert_eq!(
            server.endpoint_label(),
            r"dll:C:\LHM\LibreHardwareMonitorLib.dll"
        );
    }
}
