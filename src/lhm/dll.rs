//! Direct `LibreHardwareMonitorLib.dll` data source (SPEC §3.6).
//!
//! LHM's library is a .NET assembly. Rust cannot load it in-process without
//! CLR hosting and `unsafe` FFI (forbidden by SPEC N5), so tiny embedded
//! bridge processes load the DLL and speak a line-based stdio protocol:
//! a .NET Framework bridge for the classic LHM zip and a net8.0 bridge for
//! the .NET 8/9/10 zip. Both emit the same node tree as `/data.json`, which
//! means the existing sanitizer/parser and every UI binding stay unchanged.
//!
//! This module is compiled on every platform; DLL mode itself is Windows-only
//! and reports a clear error elsewhere.

#[cfg(windows)]
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::sync::{OnceLock, Weak};
use std::time::Instant;

use crate::config::ServerConfig;
use crate::lhm::client::{Fetched, LhmError};

/// File name looked for when `server.dll_path` points at a folder.
pub const DLL_NAME: &str = "LibreHardwareMonitorLib.dll";

/// Bridge protocol errors are stringly typed on purpose: they cross a process
/// boundary and are surfaced verbatim in the status line / `--probe`.
type BridgeResult<T> = Result<T, String>;

/// Resolve `server.dll_path` (file or folder) or auto-detect a common LHM
/// install location. The error names every path that was tried so `--probe`
/// is actionable.
pub fn resolve_dll_path(server: &ServerConfig) -> BridgeResult<PathBuf> {
    if let Some(raw) = server
        .dll_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let path = PathBuf::from(raw);
        let candidate = if path.is_dir() || path.extension().is_none() {
            path.join(DLL_NAME)
        } else {
            path
        };
        if candidate.is_file() {
            return Ok(canonical(candidate));
        }
        return Err(format!(
            "LibreHardwareMonitorLib.dll not found at `{}` — set server.dll_path to the DLL \
             or to the LibreHardwareMonitor folder",
            candidate.display()
        ));
    }

    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(DLL_NAME));
        }
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(
                PathBuf::from(root)
                    .join("LibreHardwareMonitor")
                    .join(DLL_NAME),
            );
        }
    }
    candidates.push(PathBuf::from(DLL_NAME));

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(canonical(candidate.clone()));
        }
    }

    let tried = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "cannot find {DLL_NAME}; set server.dll_path (or --dll). Tried: {tried}"
    ))
}

fn canonical(path: PathBuf) -> PathBuf {
    // `canonicalize` adds the `\\?\` extended-length prefix on Windows, which
    // .NET's Assembly.LoadFrom rejects. `absolute` keeps the path usable.
    std::path::absolute(&path).unwrap_or(path)
}

/// A live bridge process plus its pipes. One process is shared by the poller,
/// the fan-control writer and one-shot modes (keyed by the DLL path).
#[derive(Clone)]
pub struct DllClient {
    inner: Arc<DllInner>,
}

struct DllInner {
    dll_path: PathBuf,
    endpoint: String,
    process: Mutex<Option<BridgeProcess>>,
}

impl DllClient {
    /// Connect to (and start) the bridge for the configured DLL. The bridge
    /// handshake loads the assembly, so missing/incompatible DLLs fail here
    /// instead of on the first poll.
    pub fn connect(server: &ServerConfig) -> BridgeResult<Self> {
        #[cfg(not(windows))]
        {
            let _ = server;
            Err(
                "server.source = \"dll\" is Windows-only; use server.source = \"http\" on this \
                 platform"
                    .to_string(),
            )
        }

        #[cfg(windows)]
        {
            let dll_path = resolve_dll_path(server)?;
            static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Weak<DllInner>>>> = OnceLock::new();
            let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
            let mut registry = registry
                .lock()
                .map_err(|_| "DLL bridge registry lock poisoned".to_string())?;

            if let Some(existing) = registry.get(&dll_path).and_then(Weak::upgrade) {
                return Ok(Self { inner: existing });
            }

            let inner = Arc::new(DllInner {
                dll_path: dll_path.clone(),
                endpoint: format!("dll:{}", dll_path.display()),
                process: Mutex::new(None),
            });
            {
                let mut slot = inner
                    .process
                    .lock()
                    .map_err(|_| "DLL bridge lock poisoned".to_string())?;
                *slot = Some(BridgeProcess::spawn(&dll_path)?);
            }
            registry.insert(dll_path, Arc::downgrade(&inner));
            Ok(Self { inner })
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

    /// `Some(true)` when the bridge runs elevated, `Some(false)` when it is
    /// limited (CPU MSR sensors may read 0), `None` if no bridge is running.
    pub fn elevation(&self) -> Option<bool> {
        self.inner
            .process
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|process| process.elevated))
    }

    pub fn dll_path(&self) -> &Path {
        &self.inner.dll_path
    }

    fn request(&self, command: &str) -> BridgeResult<String> {
        let mut slot = self
            .inner
            .process
            .lock()
            .map_err(|_| "DLL bridge lock poisoned".to_string())?;
        if slot.is_none() {
            *slot = Some(BridgeProcess::spawn(&self.inner.dll_path)?);
        }
        let result = match slot.as_mut() {
            Some(process) => process.request(command),
            None => Err("DLL bridge is not running".to_string()),
        };
        if result.is_err() {
            // Drop the broken process; the next request respawns it.
            *slot = None;
        }
        result
    }

    /// One poll: `POLL` returns the same JSON node tree as `/data.json`.
    pub fn fetch_snapshot(&self) -> Result<Fetched, LhmError> {
        let started = Instant::now();
        let payload = self
            .request("POLL")
            .map_err(|message| self.dll_error(message))?;
        if let Some(message) = payload.strip_prefix("error ") {
            return Err(self.dll_error(message.to_string()));
        }
        let latency = started.elapsed();
        let bytes = payload.len();
        let snapshot =
            crate::lhm::parse::parse_payload(&payload).map_err(|err| LhmError::BadPayload {
                url: self.inner.endpoint.clone(),
                message: err.to_string(),
            })?;
        Ok(Fetched {
            snapshot,
            latency,
            bytes,
            elevated: self.elevation(),
        })
    }

    pub fn reset_min_max(&self) -> Result<(), LhmError> {
        let response = self
            .request("RESET")
            .map_err(|message| self.dll_error(message))?;
        self.check_result(response)
    }

    pub fn set_sensor_control(&self, sensor_id: &str, value: Option<f32>) -> Result<(), LhmError> {
        let value_text = match value {
            Some(value) => format!("{value:.1}"),
            None => "null".to_string(),
        };
        let command = format!("SET\t{sensor_id}\t{value_text}");
        let response = self
            .request(&command)
            .map_err(|message| self.dll_error(message))?;
        self.check_result(response)
    }

    fn check_result(&self, response: String) -> Result<(), LhmError> {
        if let Some(message) = response.strip_prefix("error ") {
            return Err(self.dll_error(message.to_string()));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&response).map_err(|err| LhmError::BadPayload {
                url: self.inner.endpoint.clone(),
                message: err.to_string(),
            })?;
        match parsed.get("result").and_then(|result| result.as_str()) {
            Some("ok") => Ok(()),
            Some("fail") => Err(LhmError::BadPayload {
                url: self.inner.endpoint.clone(),
                message: parsed
                    .get("message")
                    .and_then(|message| message.as_str())
                    .unwrap_or("sensor write rejected")
                    .to_string(),
            }),
            _ => Err(LhmError::BadPayload {
                url: self.inner.endpoint.clone(),
                message: "bridge response is missing a result".to_string(),
            }),
        }
    }

    fn dll_error(&self, message: String) -> LhmError {
        LhmError::Dll {
            path: self.inner.endpoint.clone(),
            message: truncate(&message.replace(['\r', '\n'], " "), 400),
        }
    }
}

#[derive(Clone, Copy)]
enum BridgeFlavor {
    /// .NET Framework bridge for the classic `LibreHardwareMonitor.zip`.
    Framework,
    /// .NET 8+ bridge for `LibreHardwareMonitor.NET.10.zip` (rolls forward).
    DotNet,
}

impl BridgeFlavor {
    fn label(self) -> &'static str {
        match self {
            BridgeFlavor::Framework => ".NET Framework",
            BridgeFlavor::DotNet => ".NET 8+",
        }
    }
}

struct BridgeProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    elevated: bool,
}

impl BridgeProcess {
    fn spawn(dll_path: &Path) -> BridgeResult<Self> {
        let assets = bridge_assets()?;
        match Self::spawn_flavor(&assets.framework_exe, BridgeFlavor::Framework, dll_path) {
            Ok(process) => Ok(process),
            Err(framework_error) => {
                // Do not launch a second UAC prompt (and a second bridge) when
                // the framework bridge already asked for elevation.
                if framework_error.contains("elevation required") {
                    return Err(framework_error);
                }
                let Some(dotnet_exe) = assets.dotnet_exe.as_ref() else {
                    return Err(format!(
                        "{framework_error} (the .NET 8+ bridge is not available in this build)"
                    ));
                };
                match Self::spawn_flavor(dotnet_exe, BridgeFlavor::DotNet, dll_path) {
                    Ok(process) => Ok(process),
                    Err(dotnet_error) => Err(format!(
                        "{framework_error}; the .NET bridge also failed: {dotnet_error}"
                    )),
                }
            }
        }
    }

    fn spawn_flavor(
        executable: &Path,
        flavor: BridgeFlavor,
        dll_path: &Path,
    ) -> BridgeResult<Self> {
        let mut command = Command::new(executable);
        command
            .arg("--dll")
            .arg(dll_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The TUI owns the terminal; bridge stderr is not useful and must
            // never be inherited (it would corrupt the screen).
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|err| {
            format!(
                "cannot start the bundled {} bridge ({}): {err}",
                flavor.label(),
                executable.display()
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "bridge stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "bridge stdout unavailable".to_string())?;
        let mut process = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            elevated: false,
        };

        let handshake = process.read_line()?;
        if let Some(ready) = handshake.strip_prefix("ready ") {
            let mut parts = ready.split_whitespace();
            let version = parts.next().unwrap_or("unknown");
            process.elevated = parts.next() == Some("elevated");
            tracing::info!(
                version,
                flavor = flavor.label(),
                elevated = process.elevated,
                dll = %dll_path.display(),
                "DLL bridge ready"
            );
            return Ok(process);
        }
        if let Some(message) = handshake.strip_prefix("error ") {
            return Err(format!(
                "{} bridge failed to load the library: {message}",
                flavor.label()
            ));
        }
        Err(format!(
            "unexpected {} bridge handshake: {}",
            flavor.label(),
            truncate(&handshake, 200)
        ))
    }

    fn request(&mut self, command: &str) -> BridgeResult<String> {
        writeln!(self.stdin, "{command}")
            .and_then(|_| self.stdin.flush())
            .map_err(|err| format!("cannot write to the DLL bridge: {err}"))?;
        self.read_line()
    }

    fn read_line(&mut self) -> BridgeResult<String> {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .map_err(|err| format!("cannot read from the DLL bridge: {err}"))?;
        if read == 0 {
            return Err("the DLL bridge exited unexpectedly".to_string());
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl Drop for BridgeProcess {
    fn drop(&mut self) {
        // Ask for a graceful Close() so LHM restores hardware defaults (fan
        // controls included), but never block shutdown indefinitely.
        let _ = writeln!(self.stdin, "QUIT");
        let _ = self.stdin.flush();
        let deadline = Instant::now() + std::time::Duration::from_millis(1000);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Embedded bridge files, extracted to a content-hashed temp directory.
#[derive(Clone)]
struct BridgeAssets {
    framework_exe: PathBuf,
    dotnet_exe: Option<PathBuf>,
}

/// Extract both embedded bridges next to the platform temp dir. The directory
/// name includes a content hash, so a rebuilt binary never reuses stale bridge
/// code.
#[cfg(windows)]
fn bridge_assets() -> BridgeResult<BridgeAssets> {
    static ASSETS: OnceLock<Result<BridgeAssets, String>> = OnceLock::new();
    ASSETS.get_or_init(extract_bridge_assets).clone()
}

#[cfg(windows)]
#[allow(clippy::const_is_empty)]
fn extract_bridge_assets() -> BridgeResult<BridgeAssets> {
    const FRAMEWORK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lhm-bridge.exe"));
    const DOTNET_EXE: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/dotnet/LhmBridgeDotNet.exe"));
    const DOTNET_DLL: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/dotnet/LhmBridgeDotNet.dll"));
    const DOTNET_RUNTIMECONFIG: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/dotnet/LhmBridgeDotNet.runtimeconfig.json"
    ));
    const DOTNET_DEPS: &[u8] = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/dotnet/LhmBridgeDotNet.deps.json"
    ));

    // `build.rs` writes empty placeholders when a compiler is unavailable;
    // clippy only sees the current build's bytes, so these checks are real.
    if FRAMEWORK.is_empty() {
        return Err(
            "this build has no DLL bridge (it was built without the .NET Framework C# compiler); \
             rebuild on Windows with csc.exe available"
                .to_string(),
        );
    }

    let dotnet_available = !DOTNET_EXE.is_empty()
        && !DOTNET_DLL.is_empty()
        && !DOTNET_RUNTIMECONFIG.is_empty()
        && !DOTNET_DEPS.is_empty();
    let hash = fnv1a_parts(&[
        FRAMEWORK,
        DOTNET_EXE,
        DOTNET_DLL,
        DOTNET_RUNTIMECONFIG,
        DOTNET_DEPS,
    ]);
    let dir = std::env::temp_dir()
        .join("lhm-tui")
        .join(format!("bridge-{hash:016x}"));
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("cannot create {}: {err}", dir.display()))?;

    let framework_exe = dir.join("lhm-bridge.exe");
    write_asset(&framework_exe, FRAMEWORK)?;
    let dotnet_exe = if dotnet_available {
        let exe = dir.join("LhmBridgeDotNet.exe");
        write_asset(&exe, DOTNET_EXE)?;
        write_asset(&dir.join("LhmBridgeDotNet.dll"), DOTNET_DLL)?;
        write_asset(
            &dir.join("LhmBridgeDotNet.runtimeconfig.json"),
            DOTNET_RUNTIMECONFIG,
        )?;
        write_asset(&dir.join("LhmBridgeDotNet.deps.json"), DOTNET_DEPS)?;
        Some(exe)
    } else {
        None
    };

    Ok(BridgeAssets {
        framework_exe,
        dotnet_exe,
    })
}

#[cfg(windows)]
fn write_asset(path: &Path, bytes: &[u8]) -> BridgeResult<()> {
    if path.is_file() {
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("asset");
    let temporary = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes)
        .map_err(|err| format!("cannot write {}: {err}", temporary.display()))?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        // Another process won the race with identical content.
        Err(_) if path.is_file() => {
            let _ = std::fs::remove_file(&temporary);
            Ok(())
        }
        Err(err) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!("cannot install {}: {err}", path.display()))
        }
    }
}

#[cfg(not(windows))]
fn bridge_assets() -> BridgeResult<BridgeAssets> {
    Err("the DLL bridge is only available on Windows".to_string())
}

#[cfg(any(windows, test))]
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(windows)]
fn fnv1a_parts(parts: &[&[u8]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        hash ^= fnv1a(part);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn explicit_missing_path_is_reported() {
        let server = ServerConfig {
            dll_path: Some("definitely-not-here/LibreHardwareMonitorLib.dll".to_string()),
            ..ServerConfig::default()
        };
        let error = resolve_dll_path(&server).expect_err("missing path must fail");
        assert!(error.contains("server.dll_path"), "{error}");
    }

    #[test]
    fn folder_path_appends_the_dll_name() {
        let dir = std::env::temp_dir().join(format!("lhm-tui-dll-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join(DLL_NAME), b"not a real assembly").expect("fixture");
        let server = ServerConfig {
            dll_path: Some(dir.display().to_string()),
            ..ServerConfig::default()
        };
        let resolved = resolve_dll_path(&server).expect("folder resolves");
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()),
            Some(DLL_NAME)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
