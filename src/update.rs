//! Explicit, stable-channel updates. Never restarts or changes open documents.
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const API: &str = "https://api.github.com/repos/zenogrid-law80/rdiff/releases/latest";
const PREFIX: &str = "https://github.com/zenogrid-law80/rdiff/releases/download/";
const ASSET: &str = "rdiff-windows-x86_64.exe";
const MAX_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Clone)]
pub struct Available {
    pub version: Version,
    asset: Asset,
    msi: bool,
}

#[cfg(test)]
fn select(json: &[u8], current: &str) -> Result<Option<Available>, String> {
    select_kind(json, current, false)
}

#[cfg(windows)]
fn powershell() -> Result<std::process::Command, String> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing.")?;
    let mut command = std::process::Command::new(
        Path::new(&system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive"])
        .creation_flags(0x08000000);
    Ok(command)
}

#[cfg(windows)]
fn msi_installation() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = powershell()?.args(["-Command", r#"
$ErrorActionPreference = 'Stop'
foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64, [Microsoft.Win32.RegistryView]::Registry32)) {
    $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, $view)
    $key = $root.OpenSubKey('Software\zenogrid-law80\rdiff')
    if ($null -ne $key) {
        $dir = $key.GetValue('InstallDirectory')
        $key.Dispose()
        if ($dir -and ([IO.Path]::GetFullPath((Join-Path $dir 'rdiff.exe')) -eq $env:RDIFF_EXE)) { $root.Dispose(); exit 10 }
    }
    $root.Dispose()
}
exit 0
"#]).env("RDIFF_EXE", exe).output().map_err(|e| e.to_string())?;
    match output.status.code() {
        Some(10) => Ok(true),
        Some(0) => Ok(false),
        _ => Err("Could not determine Windows Installer installation state.".into()),
    }
}

#[cfg(not(windows))]
fn msi_installation() -> Result<bool, String> {
    Ok(false)
}

#[cfg(windows)]
fn schedule_msi(release: &Available) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix("rdiff-msi-update-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let package = stage.path().join("update.msi");
    fetch(&release.asset.browser_download_url, &package)?;
    verify(&package, &release.asset)?;
    let script = stage.path().join("install.ps1");
    fs::write(&script, include_str!("../installer/update.ps1")).map_err(|e| e.to_string())?;
    let directory = stage.keep();
    powershell()?
        .args(["-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .env("RDIFF_EXE", exe)
        .env("RDIFF_PARENT_PID", std::process::id().to_string())
        .env("RDIFF_MSI", package)
        .env("RDIFF_MSI_SHA256", digest(&release.asset)?)
        .env("RDIFF_UPDATE_VERSION", release.version.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start MSI updater: {e}"))?;
    Ok(format!(
        "MSI update queued for v{}. Save your work and close all rdiff windows. Windows Installed apps will update after installation. Status: {}",
        release.version,
        directory.join("status.txt").display()
    ))
}

#[cfg(not(windows))]
fn schedule_msi(_: &Available) -> Result<String, String> {
    Err("MSI updates require Windows.".into())
}

fn select_kind(json: &[u8], current: &str, msi: bool) -> Result<Option<Available>, String> {
    let release: Release = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|e| format!("Invalid release version: {e}"))?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Err("The release is not a stable version.".into());
    }
    if version <= Version::parse(current).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let name = if msi {
        format!("rdiff-{version}-windows-x86_64.msi")
    } else {
        ASSET.into()
    };
    let mut assets = release.assets.into_iter().filter(|a| a.name == name);
    let asset = assets
        .next()
        .ok_or("Windows x64 release asset is missing.")?;
    if assets.next().is_some() {
        return Err("Duplicate release assets.".into());
    }
    let expected_url = format!("{PREFIX}{}/{}", release.tag_name, name);
    if asset.browser_download_url != expected_url || asset.size < 64 || asset.size > MAX_SIZE {
        return Err("Invalid release asset URL or size.".into());
    }
    digest(&asset)?;
    Ok(Some(Available {
        version,
        asset,
        msi,
    }))
}

fn digest(asset: &Asset) -> Result<&str, String> {
    asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| "Release asset has no valid GitHub SHA-256 digest.".into())
}

// Pass data through environment variables, never interpolate it into script code.
#[cfg(windows)]
fn fetch(url: &str, destination: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing.")?;
    let shell = Path::new(&system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let output = std::process::Command::new(shell)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
try {
    Invoke-WebRequest -UseBasicParsing -Uri $env:RDIFF_UPDATE_URL -OutFile $env:RDIFF_UPDATE_FILE -TimeoutSec 120 -Headers @{ 'User-Agent' = 'rdiff-self-update'; 'Accept' = 'application/vnd.github+json' } | Out-Null
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
"#])
        .env("RDIFF_UPDATE_URL", url)
        .env("RDIFF_UPDATE_FILE", destination)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "GitHub request failed (missing release, network, or rate limit): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(not(windows))]
fn fetch(_: &str, _: &Path) -> Result<(), String> {
    Err("Self-update currently supports Windows x64 only.".into())
}

pub fn check() -> Result<Option<Available>, String> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("Self-update currently supports Windows x64 only.".into());
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = temp.path().join("release.json");
    fetch(API, &path)?;
    let mut json = Vec::new();
    fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(2 * 1024 * 1024)
        .read_to_end(&mut json)
        .map_err(|e| e.to_string())?;
    select_kind(&json, VERSION, msi_installation()?)
}

fn verify(path: &Path, asset: &Asset) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != asset.size {
        return Err("Downloaded file size does not match the release.".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(digest(asset)?) {
        return Err("SHA-256 mismatch; the executable was not changed.".into());
    }
    Ok(())
}

fn replace(staged: &Path, exe: &Path, backup: &Path) -> Result<(), String> {
    fs::rename(exe, backup).map_err(|e| format!("Cannot back up executable: {e}"))?;
    if let Err(error) = fs::rename(staged, exe) {
        return match fs::rename(backup, exe) {
            Ok(()) => Err(format!(
                "Update failed; previous executable restored: {error}"
            )),
            Err(rollback) => Err(format!(
                "Update failed: {error}. Restore {} manually: {rollback}",
                backup.display()
            )),
        };
    }
    Ok(())
}

pub fn install(release: &Available) -> Result<String, String> {
    if release.msi {
        return schedule_msi(release);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("Executable directory is missing.")?;
    // OS lock is released even after a crash; keep the harmless lock file.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".rdiff-update.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "Another update is running.".to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".rdiff-update-")
        .tempdir_in(parent)
        .map_err(|e| format!("Cannot write executable directory: {e}"))?;
    let downloaded = stage.path().join("rdiff.exe");
    fetch(&release.asset.browser_download_url, &downloaded)?;
    verify(&downloaded, &release.asset)?;
    let backup = stage.path().join("previous.exe");
    // Keep the directory before touching the original, including on rollback failure.
    let backup_dir = stage.keep();
    replace(&downloaded, &exe, &backup)?;
    // Windows can keep the old image locked while existing diff/merge windows run.
    Ok(format!(
        "Installed v{}. Use the new version on next launch. Backup: {}",
        release.version,
        backup_dir.join("previous.exe").display()
    ))
}

pub fn run(check_only: bool) -> Result<String, String> {
    match check()? {
        None => Ok(format!("rdiff v{VERSION} is up to date.")),
        Some(release) if check_only => Ok(format!(
            "Update available: v{VERSION} → v{}",
            release.version
        )),
        Some(release) => install(&release),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata(tag: &str, hash: &str) -> Vec<u8> {
        serde_json::to_vec(
            &serde_json::json!({"tag_name":tag,"draft":false,"prerelease":false,
            "assets":[{"name":ASSET,"browser_download_url":format!("{PREFIX}{tag}/{ASSET}"),
                "size":64,"digest":hash}]}),
        )
        .unwrap()
    }
    #[test]
    fn msi_installation_requires_versioned_msi_asset() {
        let hash = format!("sha256:{}", "a".repeat(64));
        let exe_only = metadata("v0.2.0", &hash);
        assert!(select_kind(&exe_only, "0.1.0", true).is_err());
        let msi = String::from_utf8(exe_only)
            .unwrap()
            .replace(ASSET, "rdiff-0.2.0-windows-x86_64.msi");
        let release = select_kind(msi.as_bytes(), "0.1.0", true).unwrap().unwrap();
        assert!(release.msi);
        assert_eq!(release.asset.name, "rdiff-0.2.0-windows-x86_64.msi");
        assert!(select_kind(msi.as_bytes(), "0.1.0", false).is_err());
    }

    #[test]
    fn versions_assets_and_digest_are_validated() {
        let hash = format!("sha256:{}", "a".repeat(64));
        assert!(
            select(&metadata("v0.10.0", &hash), "0.9.0")
                .unwrap()
                .is_some()
        );
        assert!(
            select(&metadata("v0.9.0", &hash), "0.10.0")
                .unwrap()
                .is_none()
        );
        assert!(
            select(&metadata("v0.9.0", &hash), "0.9.0")
                .unwrap()
                .is_none()
        );
        assert!(select(&metadata("v1.0.0-beta.1", &hash), "0.9.0").is_err());
        assert!(select(&metadata("v1.0.0", "bad"), "0.9.0").is_err());
        let altered = String::from_utf8(metadata("v1.0.0", &hash))
            .unwrap()
            .replace("https://github.com/", "https://evil.example/");
        assert!(select(altered.as_bytes(), "0.9.0").is_err());
    }
    #[test]
    fn verifies_payload_and_restores_on_failed_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("rdiff.exe");
        let backup = temp.path().join("backup.exe");
        fs::write(&exe, b"original").unwrap();
        let asset = Asset {
            name: ASSET.into(),
            browser_download_url: String::new(),
            size: 8,
            digest: Some(format!("sha256:{:x}", Sha256::digest(b"original"))),
        };
        assert!(verify(&exe, &asset).is_ok());
        fs::write(&exe, b"tampered").unwrap();
        assert!(verify(&exe, &asset).is_err());
        assert!(replace(&temp.path().join("missing"), &exe, &backup).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"tampered");
        let staged = temp.path().join("new.exe");
        fs::write(&staged, b"new").unwrap();
        replace(&staged, &exe, &backup).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"tampered");
    }
}
