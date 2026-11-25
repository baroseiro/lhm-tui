//! Build the Windows DLL bridges:
//!
//! 1. `LhmBridge.cs` with the .NET Framework `csc.exe` that ships with
//!    Windows — loads the classic `LibreHardwareMonitor.zip` library.
//! 2. The same source with `dotnet build` (net8.0, `RollForward=LatestMajor`)
//!    — loads the `LibreHardwareMonitor.NET.10.zip` library on .NET 8/9/10.
//!
//! Both are embedded in the Rust binary and extracted on first use, so release
//! artifacts stay single-file and no .NET SDK is required on the user's
//! machine. Non-Windows builds write empty placeholders; DLL mode reports a
//! clear "Windows-only" error there (SPEC §3.6).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=bridge/LhmBridge.cs");
    println!("cargo:rerun-if-changed=bridge/LhmBridgeDotNet.csproj");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let framework_binary = out_dir.join("lhm-bridge.exe");
    let dotnet_dir = out_dir.join("dotnet");
    let dotnet_exe = dotnet_dir.join("LhmBridgeDotNet.exe");
    let dotnet_dll = dotnet_dir.join("LhmBridgeDotNet.dll");
    let dotnet_runtimeconfig = dotnet_dir.join("LhmBridgeDotNet.runtimeconfig.json");
    let dotnet_deps = dotnet_dir.join("LhmBridgeDotNet.deps.json");

    let target_windows = env::var("CARGO_CFG_TARGET_OS")
        .map(|os| os == "windows")
        .unwrap_or(false);
    if !target_windows || env::var_os("LHM_TUI_SKIP_BRIDGE").is_some() {
        if env::var_os("LHM_TUI_SKIP_BRIDGE").is_some() {
            println!("cargo:warning=LHM_TUI_SKIP_BRIDGE set; DLL mode will be unavailable");
        }
        fs::write(&framework_binary, []).expect("write empty bridge placeholder");
        write_empty_dotnet_placeholders(
            &dotnet_dir,
            &[
                &dotnet_exe,
                &dotnet_dll,
                &dotnet_runtimeconfig,
                &dotnet_deps,
            ],
        );
        return;
    }

    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = root.join("bridge").join("LhmBridge.cs");
    let project = root.join("bridge").join("LhmBridgeDotNet.csproj");

    build_framework_bridge(&source, &framework_binary);
    build_dotnet_bridge(
        &project,
        &dotnet_dir,
        &dotnet_exe,
        &dotnet_dll,
        &dotnet_runtimeconfig,
        &dotnet_deps,
    );
}

fn build_framework_bridge(source: &Path, binary: &Path) {
    let Some(csc) = csc_path() else {
        panic!(
            ".NET Framework csc.exe not found; DLL mode needs it. Install .NET Framework 4.8, \
             or set LHM_TUI_SKIP_BRIDGE=1 for an HTTP-only build"
        );
    };

    let output = Command::new(&csc)
        .arg("/nologo")
        .arg("/target:exe")
        .arg("/platform:anycpu")
        .arg("/optimize+")
        .arg("/warnaserror+")
        .arg("/langversion:5")
        .arg("/utf8output")
        .arg(format!("/out:{}", binary.display()))
        .arg(source)
        .output()
        .unwrap_or_else(|err| panic!("cannot run {}: {err}", csc.display()));

    if !output.status.success() {
        panic!(
            "failed to compile bridge/LhmBridge.cs with {}:\n{}{}",
            csc.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !binary.is_file() {
        panic!("csc succeeded but did not produce {}", binary.display());
    }
}

fn build_dotnet_bridge(
    project: &Path,
    out_dir: &Path,
    exe: &Path,
    dll: &Path,
    runtimeconfig: &Path,
    deps: &Path,
) {
    let intermediate = out_dir.join("obj");
    let result = Command::new("dotnet")
        .arg("build")
        .arg(project)
        .arg("--configuration")
        .arg("Release")
        .arg("--output")
        .arg(out_dir)
        .arg(format!(
            "-p:BaseIntermediateOutputPath={}\\",
            intermediate.display()
        ))
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .env("DOTNET_NOLOGO", "1")
        .env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1")
        .output();

    let error = match result {
        Ok(output) if output.status.success() => {
            if [exe, dll, runtimeconfig, deps]
                .iter()
                .all(|path| path.is_file())
            {
                return;
            }
            format!(
                "dotnet build did not produce all bridge files in {}",
                out_dir.display()
            )
        }
        Ok(output) => format!(
            "dotnet build failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(err) => format!("cannot run dotnet: {err}"),
    };

    println!(
        "cargo:warning=the .NET 8+ bridge was not built ({error}); DLL mode will only load the \
         .NET Framework LibreHardwareMonitor build"
    );
    write_empty_dotnet_placeholders(out_dir, &[exe, dll, runtimeconfig, deps]);
}

fn write_empty_dotnet_placeholders(out_dir: &Path, files: &[&Path]) {
    fs::create_dir_all(out_dir).expect("create dotnet bridge placeholder dir");
    for file in files {
        fs::write(file, []).expect("write empty dotnet bridge placeholder");
    }
}

/// `csc.exe` lives next to the .NET Framework installation. Prefer 64-bit so
/// the bridge is AnyCPU and works on both Windows x64 and ARM64.
fn csc_path() -> Option<PathBuf> {
    let windir = env::var_os("WINDIR")?;
    for framework in ["Framework64", "Framework"] {
        let candidate = Path::new(&windir)
            .join("Microsoft.NET")
            .join(framework)
            .join("v4.0.30319")
            .join("csc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
