//! Utilities: file logging with size-based rotation (SPEC §10.2).
//!
//! The TUI owns stdout, so logs always go to a file — default
//! `%TEMP%\lhm-tui.log` on Windows, `$XDG_STATE_HOME/lhm-tui/lhm-tui.log`
//! elsewhere. Rotation is 10 MB per file, keeping three files total.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

const LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
const LOG_KEEP: usize = 3;

/// Resolve the log file path: explicit `--log`, else the platform default.
pub fn resolve_log_path(override_path: Option<&Path>) -> PathBuf {
    if let Some(path) = override_path {
        return path.to_path_buf();
    }
    #[cfg(windows)]
    {
        std::env::temp_dir().join("lhm-tui.log")
    }
    #[cfg(not(windows))]
    {
        directories::BaseDirs::new()
            .and_then(|base| {
                base.state_dir()
                    .map(|dir| dir.join("lhm-tui").join("lhm-tui.log"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("lhm-tui.log"))
    }
}

/// Human-readable binary byte size, e.g. `"182.4 KB"`.
pub fn human_bytes(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Install the file logger. Verbosity: 0 = info, 1 = debug, 2+ = trace;
/// `RUST_LOG` overrides the built level when set and valid.
pub fn init_logging(path: &Path, verbosity: u8) -> anyhow::Result<()> {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| EnvFilter::new(level));

    let writer = RotatingWriter::open(path, LOG_MAX_BYTES, LOG_KEEP)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to install the tracing subscriber: {err}"))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RotatingWriter {
    inner: Arc<Mutex<Inner>>,
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
}

#[derive(Debug)]
struct Inner {
    file: File,
    written: u64,
}

impl RotatingWriter {
    /// Open (or create) the log file. `keep` counts the current file plus backups.
    pub fn open(path: &Path, max_bytes: u64, keep: usize) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let (file, written) = open_append(path)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner { file, written })),
            path: path.to_path_buf(),
            max_bytes,
            keep: keep.max(1),
        })
    }

    fn rotated_path(&self, index: usize) -> PathBuf {
        self.path.with_extension(format!("log.{index}"))
    }

    fn rotate_locked(&self, inner: &mut Inner) -> io::Result<()> {
        inner.file.flush()?;
        if self.keep > 1 {
            for index in (1..self.keep - 1).rev() {
                let from = self.rotated_path(index);
                if from.exists() {
                    std::fs::rename(&from, self.rotated_path(index + 1))?;
                }
            }
            std::fs::rename(&self.path, self.rotated_path(1))?;
        }
        let (file, _) = open_append(&self.path)?;
        inner.file = file;
        inner.written = 0;
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<(File, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let written = file.metadata()?.len();
    Ok((file, written))
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.written > 0 && inner.written + buf.len() as u64 > self.max_bytes {
            self.rotate_locked(&mut inner)?;
        }
        let written = inner.file.write(buf)?;
        inner.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.file.flush()
    }
}

impl MakeWriter<'_> for RotatingWriter {
    type Writer = RotatingWriter;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lhm-tui-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn rotates_and_keeps_backups() {
        let dir = unique_dir("logs");
        let path = dir.join("lhm-tui.log");
        let mut writer = RotatingWriter::open(&path, 64, 3).expect("open writer");
        for line in 0..12 {
            writeln!(writer, "line-{line:02} xxxxxxxxxxxxxxxxxxxx").expect("write line");
        }
        writer.flush().expect("flush");

        assert!(path.exists(), "current log exists");
        assert!(dir.join("lhm-tui.log.1").exists(), "first backup exists");
        assert!(dir.join("lhm-tui.log.2").exists(), "second backup exists");
        assert!(
            !dir.join("lhm-tui.log.3").exists(),
            "keep=3 means no third backup"
        );

        let current = std::fs::read_to_string(&path).expect("read current");
        assert!(current.contains("line-11"), "current: {current:?}");
    }

    #[test]
    fn appends_to_existing_file() {
        let dir = unique_dir("append");
        let path = dir.join("lhm-tui.log");
        std::fs::write(&path, "old\n").expect("seed");
        let mut writer = RotatingWriter::open(&path, 1024, 3).expect("open");
        writeln!(writer, "new").expect("write");
        writer.flush().expect("flush");
        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(content, "old\nnew\n");
    }
}
