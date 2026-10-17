//! Poller task behavior (PLAN T1.5): failure→backoff→recovery transitions.

mod support;

use lhm_tui::app::{spawn_poller, PollEvent};
use lhm_tui::config::{RetryConfig, ServerConfig};
use support::{MockLhm, Step};

#[tokio::test]
async fn recovers_after_failures_with_backoff() {
    let mock = MockLhm::start(vec![
        Step::status(500),
        Step::status(500),
        Step::status(500),
        Step::json(support::fixture("lhm-0.9.6.json")),
    ]);

    let server = ServerConfig {
        url: mock.url(),
        poll_interval_ms: 50,
        request_timeout_ms: 1000,
        retry: RetryConfig {
            initial_ms: 60,
            max_ms: 400,
            factor: 2.0,
            jitter: 0.0,
        },
        ..ServerConfig::default()
    };

    let mut events = spawn_poller(&server);
    let mut attempts: Vec<u32> = Vec::new();
    let mut delays_ms: Vec<u128> = Vec::new();
    let mut saw_snapshot = false;

    for _ in 0..64 {
        if events.changed().await.is_err() {
            break;
        }
        let event = events.borrow_and_update().clone();
        match event {
            PollEvent::Starting => {}
            PollEvent::Failure {
                attempt, retry_in, ..
            } => {
                attempts.push(attempt);
                delays_ms.push(retry_in.as_millis());
            }
            PollEvent::Snapshot(fetched) => {
                assert_eq!(fetched.snapshot.sensor_count(), 22);
                saw_snapshot = true;
                break;
            }
        }
    }

    assert!(saw_snapshot, "poller recovered and delivered a snapshot");
    assert_eq!(attempts, vec![0, 1, 2], "consecutive failure numbering");
    assert_eq!(
        delays_ms,
        vec![60, 120, 240],
        "exponential backoff schedule"
    );
    assert!(mock.request_count() >= 4, "three failures plus a success");
}

#[tokio::test]
async fn backoff_delay_caps_at_max() {
    let mock = MockLhm::start(vec![Step::status(500)]);
    let server = ServerConfig {
        url: mock.url(),
        poll_interval_ms: 30,
        retry: RetryConfig {
            initial_ms: 100,
            max_ms: 250,
            factor: 2.0,
            jitter: 0.0,
        },
        ..ServerConfig::default()
    };

    let mut events = spawn_poller(&server);
    let mut delays_ms: Vec<u128> = Vec::new();
    for _ in 0..4 {
        if events.changed().await.is_err() {
            break;
        }
        if let PollEvent::Failure { retry_in, .. } = events.borrow_and_update().clone() {
            delays_ms.push(retry_in.as_millis());
        }
    }

    assert!(
        delays_ms.len() >= 3,
        "at least three failures observed: {delays_ms:?}"
    );
    assert_eq!(&delays_ms[..3], &[100, 200, 250], "capped at max_ms");
}
