//! Shared test support: fixture loading and a scripted mock LHM server.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Load a fixture from `tests/fixtures/`.
pub fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read fixture {}: {err}", path.display()))
}

/// One scripted HTTP response.
#[derive(Debug, Clone)]
pub struct Step {
    pub status: u16,
    pub body: String,
    pub delay: Duration,
    /// When set, requests without these Basic credentials receive 401.
    pub require_auth: Option<(String, String)>,
}

impl Step {
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            delay: Duration::ZERO,
            require_auth: None,
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: format!("{{\"result\":\"fail\",\"code\":{status}}}"),
            delay: Duration::ZERO,
            require_auth: None,
        }
    }

    pub fn delay_for(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn require_auth(mut self, user: &str, pass: &str) -> Self {
        self.require_auth = Some((user.to_string(), pass.to_string()));
        self
    }
}

/// A tiny scripted HTTP server that impersonates LHM's web server.
/// Steps are consumed in order; after the script ends the last step repeats.
pub struct MockLhm {
    addr: SocketAddr,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicUsize>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MockLhm {
    pub fn start(steps: Vec<Step>) -> Self {
        assert!(!steps.is_empty(), "mock needs at least one step");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().expect("mock addr");
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicUsize::new(0));

        let handle = {
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut index = 0usize;
                while stop.load(Ordering::SeqCst) == 0 {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // Accepted sockets inherit the listener's non-blocking
                            // flag (Winsock); restore blocking reads for this connection.
                            let _ = stream.set_nonblocking(false);
                            let request = read_request(&mut stream);
                            let step = steps
                                .get(index)
                                .cloned()
                                .unwrap_or_else(|| steps.last().cloned().expect("steps non-empty"));
                            index += 1;
                            requests.fetch_add(1, Ordering::SeqCst);

                            let authorized = match &step.require_auth {
                                None => true,
                                Some((user, pass)) => {
                                    let expected = format!(
                                        "authorization: basic {}",
                                        base64(format!("{user}:{pass}")).to_ascii_lowercase()
                                    );
                                    request.to_ascii_lowercase().contains(&expected)
                                }
                            };

                            if !step.delay.is_zero() {
                                std::thread::sleep(step.delay);
                            }
                            let (status, reason, body) = if authorized {
                                (step.status, reason_phrase(step.status), step.body.clone())
                            } else {
                                (401, "Unauthorized", "401 Unauthorized".to_string())
                            };
                            let response = format!(
                                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        Self {
            addr,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn stop(&mut self) {
        self.stop.store(1, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MockLhm {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1000)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Minimal base64 encoder for the `Authorization: Basic` check.
fn base64(input: String) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_examples() {
        assert_eq!(base64("".to_string()), "");
        assert_eq!(base64("f".to_string()), "Zg==");
        assert_eq!(base64("fo".to_string()), "Zm8=");
        assert_eq!(base64("foo".to_string()), "Zm9v");
        assert_eq!(base64("user:s3cret".to_string()), "dXNlcjpzM2NyZXQ=");
    }
}
