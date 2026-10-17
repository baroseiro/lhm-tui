//! Client behavior against the scripted mock LHM server (PLAN T1.4/T1.5):
//! success, auth, HTTP errors, garbage payloads, timeouts and Basic auth.

mod support;

use std::time::Duration;

use lhm_tui::config::ServerConfig;
use lhm_tui::lhm::client::{LhmClient, LhmError};
use support::{MockLhm, Step};

fn config_for(url: &str) -> ServerConfig {
    ServerConfig {
        url: url.to_string(),
        request_timeout_ms: 2000,
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn fetches_and_parses_data_json() {
    let mock = MockLhm::start(vec![Step::json(support::fixture("lhm-0.9.6.json"))]);
    let client = LhmClient::new(&config_for(&mock.url())).expect("client");

    let fetched = client.fetch_data_json().await.expect("fetch");
    assert_eq!(fetched.snapshot.lhm_version.as_deref(), Some("0.9.6"));
    assert_eq!(fetched.snapshot.sensor_count(), 22);
    assert!(fetched.bytes > 1000, "payload size recorded");
    assert_eq!(mock.request_count(), 1);
}

#[tokio::test]
async fn maps_401_to_auth_error() {
    let mock = MockLhm::start(vec![Step::status(401)]);
    let client = LhmClient::new(&config_for(&mock.url())).expect("client");

    let err = client.fetch_data_json().await.expect_err("must fail");
    assert!(matches!(err, LhmError::Auth { .. }), "got: {err}");
}

#[tokio::test]
async fn maps_500_to_http_error() {
    let mock = MockLhm::start(vec![Step::status(500)]);
    let client = LhmClient::new(&config_for(&mock.url())).expect("client");

    let err = client.fetch_data_json().await.expect_err("must fail");
    assert!(
        matches!(err, LhmError::Http { status: 500, .. }),
        "got: {err}"
    );
}

#[tokio::test]
async fn maps_garbage_to_bad_payload() {
    let mock = MockLhm::start(vec![Step::json("<!doctype html><html>not json</html>")]);
    let client = LhmClient::new(&config_for(&mock.url())).expect("client");

    let err = client.fetch_data_json().await.expect_err("must fail");
    assert!(matches!(err, LhmError::BadPayload { .. }), "got: {err}");
}

#[tokio::test]
async fn maps_timeout_to_connect_error() {
    let mock = MockLhm::start(vec![Step::json("{}").delay_for(Duration::from_millis(600))]);
    let mut server = config_for(&mock.url());
    server.request_timeout_ms = 150;
    let client = LhmClient::new(&server).expect("client");

    let err = client.fetch_data_json().await.expect_err("must fail");
    assert!(matches!(err, LhmError::Connect { .. }), "got: {err}");
}

#[tokio::test]
async fn sends_basic_auth_when_configured() {
    let mock = MockLhm::start(vec![
        Step::json(support::fixture("lhm-ram-only.json")).require_auth("user", "s3cret")
    ]);

    let mut server = config_for(&mock.url());
    server.username = Some("user".to_string());
    server.password = Some("s3cret".to_string());
    let client = LhmClient::new(&server).expect("client");
    let fetched = client.fetch_data_json().await.expect("authorized fetch");
    assert_eq!(fetched.snapshot.hardware_count(), 1);

    // Same mock, wrong credentials → 401 mapped to Auth.
    let mut wrong = config_for(&mock.url());
    wrong.username = Some("user".to_string());
    wrong.password = Some("wrong".to_string());
    let client = LhmClient::new(&wrong).expect("client");
    let err = client.fetch_data_json().await.expect_err("must fail");
    assert!(matches!(err, LhmError::Auth { .. }), "got: {err}");
}
