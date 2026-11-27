//! App updates: checking `latest.json`, downloading and verifying a release
//! archive by SHA-256, and installing + relaunching on Windows, macOS, and
//! Linux (while cleanly reporting package-managed installs like `/nix/store`).

use crate::net::{self, NetError};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Where a release publishes its version and platform assets.
const LATEST_URL: &str = "https://assets.textures.gg/studio/latest.json";
/// Where the player gets it when falling back to the browser.
pub const DOWNLOAD_URL: &str = "https://textures.gg/download";
/// Maximum size of `latest.json`.
const MANIFEST_LIMIT: u64 = 32 * 1024;
/// Maximum download size for an app installer/archive (256 MiB).
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;

/// A published release from `latest.json`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub released: Option<String>,
    #[serde(default)]
    pub notes_url: Option<String>,
    #[serde(default)]
    pub assets: Assets,
}

/// Platform release files published alongside a [`Release`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Assets {
    #[serde(default)]
    pub windows: Vec<Asset>,
    #[serde(default)]
    pub macos: Vec<Asset>,
    #[serde(default)]
    pub linux: Vec<Asset>,
}

/// One downloadable release artifact.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Asset {
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub min_os: Option<String>,
}

/// How this copy of `tgg-studio` can be updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallMethod {
    /// Windows Inno Setup install (`unins000.exe` beside the binary).
    WindowsInstaller,
    /// Windows portable Zip install (in-place exe swap via `.old` rename).
    WindowsZip,
    /// macOS writable `.app` bundle (mount DMG, `ditto`, atomic swap, relaunch).
    MacAppBundle(PathBuf),
    /// macOS non-bundle or read-only location (download DMG and open in Finder).
    MacDmg,
    /// Linux user install in `~/.local/bin/tgg-studio` (atomic binary swap + desktop files).
    LinuxLocalInstall(PathBuf),
    /// Linux user-writable standalone binary (atomic rename over exe + relaunch).
    LinuxBinary(PathBuf),
    /// Managed externally (Nix store, system package manager, or read-only dir).
    Managed { reason: String },
}

impl InstallMethod {
    /// Whether the app can download and install the update in-app.
    pub fn can_install(&self) -> bool {
        !matches!(self, Self::Managed { .. })
    }

    /// Whether installing restarts the app automatically.
    pub fn restarts(&self) -> bool {
        !matches!(self, Self::Managed { .. } | Self::MacDmg)
    }

    /// Why this copy can't update itself in place, when it's externally managed.
    pub fn managed_reason(&self) -> Option<&str> {
        match self {
            Self::Managed { reason } => Some(reason),
            _ => None,
        }
    }
}

/// What happened once the downloaded release was handed off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// The updated app (or its installer) is launching; this process should quit.
    Relaunching,
    /// Opened an installer image (e.g. macOS DMG) for the user to finish.
    OpenedInstaller,
}

/// Where an in-progress update is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Downloading,
    Verifying,
    Installing,
}

/// Live progress reported during download, verification, and install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub bytes: u64,
    pub total: Option<u64>,
}

/// Why an in-app update couldn't complete.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Update cancelled.")]
    Cancelled,
    #[error("No download is available for this platform.")]
    NoAsset,
    #[error("{0}")]
    Managed(String),
    #[error("Couldn't download the update: {0}")]
    Network(String),
    #[error("The downloaded file didn't match its checksum.")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("Couldn't write the update: {0}")]
    Io(String),
    #[error("Couldn't install the update: {0}")]
    Install(String),
}

impl Release {
    /// The release asset matching `method`, if `latest.json` published one.
    pub fn asset_for(&self, method: &InstallMethod) -> Option<&Asset> {
        match method {
            InstallMethod::WindowsInstaller => self
                .assets
                .windows
                .iter()
                .find(|a| a.kind.eq_ignore_ascii_case("installer"))
                .or_else(|| self.assets.windows.first()),
            InstallMethod::WindowsZip => self
                .assets
                .windows
                .iter()
                .find(|a| a.kind.eq_ignore_ascii_case("zip"))
                .or_else(|| self.assets.windows.first()),
            InstallMethod::MacAppBundle(_) | InstallMethod::MacDmg => self
                .assets
                .macos
                .iter()
                .find(|a| a.kind.eq_ignore_ascii_case("dmg"))
                .or_else(|| self.assets.macos.first()),
            InstallMethod::LinuxLocalInstall(_) | InstallMethod::LinuxBinary(_) => self
                .assets
                .linux
                .iter()
                .find(|a| a.kind.eq_ignore_ascii_case("tar.gz"))
                .or_else(|| self.assets.linux.first()),
            InstallMethod::Managed { .. } => None,
        }
    }
}

/// The latest version's file, or the one `TGG_UPDATE_URL` names.
fn latest_url() -> String {
    std::env::var("TGG_UPDATE_URL").unwrap_or_else(|_| LATEST_URL.into())
}

/// The release published at `url` when it is newer than `current`.
fn check_at(url: &str, current: &str) -> Result<Option<Release>, NetError> {
    let body = net::get(url, MANIFEST_LIMIT)?;
    let latest: Release = serde_json::from_str(&body)?;
    Ok(newer(&latest.version, current).then_some(latest))
}

/// A newer release than `current` (the app's), when one is published.
/// Returns errors to callers that want to show manual check feedback.
pub fn check_result(current: &str) -> Result<Option<Release>, NetError> {
    check_at(&latest_url(), current)
}

/// A newer release than `current` (the app's), when one is published. A failed
/// launch check is logged and is no news.
pub fn check(current: &str) -> Option<Release> {
    check_result(current)
        .inspect_err(|error| crate::log(&format!("update check failed: {error}")))
        .ok()
        .flatten()
}

/// Whether `latest` is a later `major.minor.patch` than `current`. A
/// version that isn't three numbers is never newer.
pub fn newer(latest: &str, current: &str) -> bool {
    fn parts(version: &str) -> Option<[u64; 3]> {
        let mut numbers = version.split('.').map(|part| part.parse().ok());
        let parts = [numbers.next()??, numbers.next()??, numbers.next()??];
        numbers.next().is_none().then_some(parts)
    }
    match (parts(latest), parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// Remove a leftover `.old` executable left behind by a prior in-place update.
pub fn cleanup_old_binary() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let old = old_binary_path(&exe);
    if old.exists() {
        let _ = fs::remove_file(old);
    }
}

fn old_binary_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// How the running executable can update itself.
pub fn detect_install_method() -> InstallMethod {
    match std::env::current_exe() {
        Ok(exe) => detect_install_method_for(&exe),
        Err(error) => InstallMethod::Managed {
            reason: format!("Couldn't locate the running app ({error})."),
        },
    }
}

fn detect_install_method_for(exe: &Path) -> InstallMethod {
    #[cfg(target_os = "windows")]
    {
        detect_windows(exe)
    }
    #[cfg(target_os = "macos")]
    {
        detect_macos(exe)
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux(exe, dirs::home_dir().as_deref())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = exe;
        InstallMethod::Managed {
            reason: "In-app updates aren't supported on this operating system.".into(),
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn detect_windows(exe: &Path) -> InstallMethod {
    let Some(dir) = exe.parent() else {
        return InstallMethod::WindowsInstaller;
    };
    if dir.join("unins000.exe").is_file() {
        return InstallMethod::WindowsInstaller;
    }
    if dir_is_writable(dir) {
        InstallMethod::WindowsZip
    } else {
        InstallMethod::WindowsInstaller
    }
}

#[cfg(any(target_os = "macos", test))]
fn detect_macos(exe: &Path) -> InstallMethod {
    if exe.starts_with("/nix/store") {
        return InstallMethod::Managed {
            reason: "This copy is managed by Nix (/nix/store). Update your Nix package to install the new version.".into(),
        };
    }
    if let Some(bundle) = mac_app_bundle(exe)
        && bundle.parent().is_some_and(dir_is_writable)
    {
        return InstallMethod::MacAppBundle(bundle);
    }
    InstallMethod::MacDmg
}

#[cfg(any(target_os = "macos", test))]
fn mac_app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    if macos.file_name().and_then(|n| n.to_str()) != Some("MacOS") {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name().and_then(|n| n.to_str()) != Some("Contents") {
        return None;
    }
    let bundle = contents.parent()?;
    (bundle.extension().and_then(|e| e.to_str()) == Some("app")).then(|| bundle.to_path_buf())
}

#[cfg(any(target_os = "linux", test))]
fn detect_linux(exe: &Path, home: Option<&Path>) -> InstallMethod {
    let resolved = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    if resolved.starts_with("/nix/store") || exe.starts_with("/nix/store") {
        return InstallMethod::Managed {
            reason: "This copy is managed by Nix (/nix/store). Update your Nix package and rebuild to install the new version.".into(),
        };
    }
    for prefix in ["/usr", "/opt", "/snap", "/app"] {
        if resolved.starts_with(prefix) {
            return InstallMethod::Managed {
                reason: "This copy is managed by your system package manager and can't update itself in place.".into(),
            };
        }
    }
    let Some(parent) = exe.parent() else {
        return InstallMethod::Managed {
            reason: "This copy can't update itself in place.".into(),
        };
    };
    if !dir_is_writable(parent) {
        return InstallMethod::Managed {
            reason: "This copy is in a read-only folder and can't update itself in place.".into(),
        };
    }
    if let Some(home) = home
        && exe == home.join(".local/bin/tgg-studio")
    {
        return InstallMethod::LinuxLocalInstall(exe.to_path_buf());
    }
    InstallMethod::LinuxBinary(exe.to_path_buf())
}

fn dir_is_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(".tgg-update-probe-{}", std::process::id()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Download `asset` into `dest_dir`, verify its SHA-256 checksum, and return
/// the verified file's path.
pub fn download_and_verify(
    asset: &Asset,
    dest_dir: &Path,
    on_progress: &dyn Fn(Progress),
    cancel: &AtomicBool,
) -> Result<PathBuf, UpdateError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(UpdateError::Cancelled);
    }
    fs::create_dir_all(dest_dir).map_err(|e| UpdateError::Io(e.to_string()))?;

    let filename = asset
        .url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && !s.contains('?') && !s.contains('\\'))
        .unwrap_or("textures.gg-update.bin");
    let final_path = dest_dir.join(filename);
    let part_path = dest_dir.join(format!("{filename}.part"));

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .user_agent(concat!("textures.gg-editor/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let mut response = agent
        .get(&asset.url)
        .call()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    let header_total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let total = header_total.or_else(|| (asset.size > 0).then_some(asset.size));

    on_progress(Progress {
        stage: Stage::Downloading,
        bytes: 0,
        total,
    });

    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(&part_path).map_err(|e| UpdateError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = fs::remove_file(&part_path);
            return Err(UpdateError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            UpdateError::Network(e.to_string())
        })?;
        if n == 0 {
            break;
        }
        downloaded = downloaded.saturating_add(n as u64);
        if downloaded > DOWNLOAD_LIMIT {
            drop(file);
            let _ = fs::remove_file(&part_path);
            return Err(UpdateError::Network(
                "update download exceeded maximum size".into(),
            ));
        }
        file.write_all(&buf[..n]).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            UpdateError::Io(e.to_string())
        })?;
        hasher.update(&buf[..n]);
        on_progress(Progress {
            stage: Stage::Downloading,
            bytes: downloaded,
            total,
        });
    }
    file.flush().map_err(|e| UpdateError::Io(e.to_string()))?;
    drop(file);

    on_progress(Progress {
        stage: Stage::Verifying,
        bytes: downloaded,
        total: total.or(Some(downloaded)),
    });

    let actual = format!("{:x}", hasher.finalize());
    if !asset.sha256.is_empty() && !actual.eq_ignore_ascii_case(&asset.sha256) {
        let _ = fs::remove_file(&part_path);
        return Err(UpdateError::ChecksumMismatch {
            expected: asset.sha256.clone(),
            actual,
        });
    }

    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&part_path, &final_path).map_err(|e| UpdateError::Io(e.to_string()))?;
    Ok(final_path)
}

/// Download, verify, and install `release` using `method`.
pub fn install_release(
    release: &Release,
    method: &InstallMethod,
    on_progress: &dyn Fn(Progress),
    cancel: &AtomicBool,
) -> Result<ApplyOutcome, UpdateError> {
    if let InstallMethod::Managed { reason } = method {
        return Err(UpdateError::Managed(reason.clone()));
    }
    let asset = release.asset_for(method).ok_or(UpdateError::NoAsset)?;
    let work_dir = std::env::temp_dir().join(format!(
        "textures.gg-update-{}-{}",
        release.version,
        std::process::id()
    ));
    let archive = download_and_verify(asset, &work_dir, on_progress, cancel)?;
    if cancel.load(Ordering::Relaxed) {
        let _ = fs::remove_dir_all(&work_dir);
        return Err(UpdateError::Cancelled);
    }
    on_progress(Progress {
        stage: Stage::Installing,
        bytes: asset.size,
        total: (asset.size > 0).then_some(asset.size),
    });
    let current_exe = std::env::current_exe().map_err(|e| UpdateError::Install(e.to_string()))?;
    apply_and_relaunch(method, &archive, &work_dir, &current_exe)
}

fn apply_and_relaunch(
    method: &InstallMethod,
    archive: &Path,
    work_dir: &Path,
    current_exe: &Path,
) -> Result<ApplyOutcome, UpdateError> {
    let _ = (archive, work_dir, current_exe);
    match method {
        InstallMethod::Managed { reason } => Err(UpdateError::Managed(reason.clone())),
        #[cfg(target_os = "windows")]
        InstallMethod::WindowsInstaller => apply_windows_installer(archive, work_dir, current_exe),
        #[cfg(target_os = "windows")]
        InstallMethod::WindowsZip => apply_windows_zip(archive, work_dir, current_exe),
        #[cfg(target_os = "macos")]
        InstallMethod::MacAppBundle(bundle) => apply_macos_bundle(archive, work_dir, bundle),
        #[cfg(target_os = "macos")]
        InstallMethod::MacDmg => {
            std::process::Command::new("open")
                .arg(archive)
                .spawn()
                .map_err(|e| UpdateError::Install(e.to_string()))?;
            Ok(ApplyOutcome::OpenedInstaller)
        }
        #[cfg(target_os = "linux")]
        InstallMethod::LinuxLocalInstall(exe) => apply_linux_tar_gz(archive, work_dir, exe, true),
        #[cfg(target_os = "linux")]
        InstallMethod::LinuxBinary(exe) => apply_linux_tar_gz(archive, work_dir, exe, false),
        #[allow(unreachable_patterns)]
        _ => Err(UpdateError::Install(
            "Install method doesn't match this operating system.".into(),
        )),
    }
}

#[cfg(target_os = "windows")]
fn apply_windows_installer(
    setup_exe: &Path,
    work_dir: &Path,
    current_exe: &Path,
) -> Result<ApplyOutcome, UpdateError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script_path = work_dir.join("run-update.cmd");
    let script = format!(
        "@echo off\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         \"{}\" /SP- /SILENT /CLOSEAPPLICATIONS\r\n\
         if errorlevel 1 exit /b %errorlevel%\r\n\
         start \"\" \"{}\"\r\n",
        setup_exe.display(),
        current_exe.display()
    );
    fs::write(&script_path, script).map_err(|e| UpdateError::Io(e.to_string()))?;
    std::process::Command::new("cmd")
        .args(["/C", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    Ok(ApplyOutcome::Relaunching)
}

#[cfg(any(target_os = "windows", test))]
fn extract_zip_binary(archive: &Path, stage: &Path) -> Result<PathBuf, UpdateError> {
    let file = File::open(archive).map_err(|e| UpdateError::Io(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| UpdateError::Install(e.to_string()))?;
    fs::create_dir_all(stage).map_err(|e| UpdateError::Io(e.to_string()))?;
    let mut found = None;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| UpdateError::Install(e.to_string()))?;
        let Some(rel) = safe_rel_path(Path::new(entry.name())) else {
            continue;
        };
        let out = stage.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| UpdateError::Io(e.to_string()))?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| UpdateError::Io(e.to_string()))?;
            }
            let mut dest = File::create(&out).map_err(|e| UpdateError::Io(e.to_string()))?;
            io::copy(&mut entry, &mut dest).map_err(|e| UpdateError::Io(e.to_string()))?;
            if out.file_name().and_then(|n| n.to_str()) == Some("tgg-studio.exe") {
                found = Some(out);
            }
        }
    }
    found.ok_or_else(|| UpdateError::Install("Zip archive didn't contain tgg-studio.exe.".into()))
}

#[cfg(target_os = "windows")]
fn apply_windows_zip(
    archive: &Path,
    work_dir: &Path,
    current_exe: &Path,
) -> Result<ApplyOutcome, UpdateError> {
    let stage = work_dir.join("stage");
    let new_exe = extract_zip_binary(archive, &stage)?;
    let old_exe = old_binary_path(current_exe);
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }
    fs::rename(current_exe, &old_exe).map_err(|e| UpdateError::Install(e.to_string()))?;
    if let Err(error) = fs::copy(&new_exe, current_exe) {
        let _ = fs::rename(&old_exe, current_exe);
        return Err(UpdateError::Install(error.to_string()));
    }
    let _ = fs::remove_dir_all(work_dir);
    std::process::Command::new(current_exe)
        .spawn()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    Ok(ApplyOutcome::Relaunching)
}

#[cfg(target_os = "macos")]
fn apply_macos_bundle(
    dmg: &Path,
    work_dir: &Path,
    bundle: &Path,
) -> Result<ApplyOutcome, UpdateError> {
    let mount_dir = work_dir.join("mount");
    fs::create_dir_all(&mount_dir).map_err(|e| UpdateError::Io(e.to_string()))?;
    let status = std::process::Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-quiet", "-mountpoint"])
        .arg(&mount_dir)
        .arg(dmg)
        .status()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    if !status.success() {
        return Err(UpdateError::Install(
            "Couldn't mount the update disk image.".into(),
        ));
    }
    struct Unmount(PathBuf);
    impl Drop for Unmount {
        fn drop(&mut self) {
            let _ = std::process::Command::new("hdiutil")
                .args(["detach", "-quiet"])
                .arg(&self.0)
                .status();
        }
    }
    let _guard = Unmount(mount_dir.clone());

    let src_app = fs::read_dir(&mount_dir)
        .map_err(|e| UpdateError::Install(e.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .ok_or_else(|| {
            UpdateError::Install("The disk image didn't contain textures.gg.app.".into())
        })?;

    let parent = bundle
        .parent()
        .ok_or_else(|| UpdateError::Install("Couldn't find Applications folder.".into()))?;
    let staged = parent.join(format!(".textures.gg-update-{}.app", std::process::id()));
    let backup = parent.join(format!(".textures.gg-old-{}.app", std::process::id()));
    if staged.exists() {
        let _ = fs::remove_dir_all(&staged);
    }
    let ditto = std::process::Command::new("ditto")
        .arg(&src_app)
        .arg(&staged)
        .status()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    if !ditto.success() {
        let _ = fs::remove_dir_all(&staged);
        return Err(UpdateError::Install(
            "Couldn't copy the updated app.".into(),
        ));
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    fs::rename(bundle, &backup).map_err(|e| {
        let _ = fs::remove_dir_all(&staged);
        UpdateError::Install(e.to_string())
    })?;
    if let Err(error) = fs::rename(&staged, bundle) {
        let _ = fs::rename(&backup, bundle);
        return Err(UpdateError::Install(error.to_string()));
    }
    let _ = fs::remove_dir_all(&backup);
    drop(_guard);
    let _ = fs::remove_dir_all(work_dir);

    std::process::Command::new("sh")
        .args(["-c", "sleep 0.5; open -n \"$1\"", "_"])
        .arg(bundle)
        .spawn()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    Ok(ApplyOutcome::Relaunching)
}

#[cfg(any(target_os = "linux", test))]
fn extract_tar_gz_binary(archive: &Path, stage: &Path) -> Result<(PathBuf, PathBuf), UpdateError> {
    let file = File::open(archive).map_err(|e| UpdateError::Io(e.to_string()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    fs::create_dir_all(stage).map_err(|e| UpdateError::Io(e.to_string()))?;
    let mut found = None;
    for entry in tar
        .entries()
        .map_err(|e| UpdateError::Install(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| UpdateError::Install(e.to_string()))?;
        let raw_path = entry
            .path()
            .map_err(|e| UpdateError::Install(e.to_string()))?
            .into_owned();
        let Some(rel) = safe_rel_path(&raw_path) else {
            continue;
        };
        let out = stage.join(&rel);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out).map_err(|e| UpdateError::Io(e.to_string()))?;
        } else if entry.header().entry_type().is_file() {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| UpdateError::Io(e.to_string()))?;
            }
            let mut dest = File::create(&out).map_err(|e| UpdateError::Io(e.to_string()))?;
            io::copy(&mut entry, &mut dest).map_err(|e| UpdateError::Io(e.to_string()))?;
            if out.file_name().and_then(|n| n.to_str()) == Some("tgg-studio") {
                found = Some(out);
            }
        }
    }
    let binary =
        found.ok_or_else(|| UpdateError::Install("Archive didn't contain tgg-studio.".into()))?;
    let root = binary.parent().unwrap_or(stage).to_path_buf();
    Ok((binary, root))
}

#[cfg(any(target_os = "linux", test))]
fn replace_linux_binary(new_binary: &Path, dest: &Path) -> Result<(), UpdateError> {
    let dir = dest
        .parent()
        .ok_or_else(|| UpdateError::Install("Couldn't find binary folder.".into()))?;
    let staged = dir.join(format!(".tgg-studio.new-{}", std::process::id()));
    fs::copy(new_binary, &staged).map_err(|e| UpdateError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
            .map_err(|e| UpdateError::Io(e.to_string()))?;
    }
    // POSIX rename replaces the directory entry atomically even while the old
    // binary is executing, avoiding ETXTBSY.
    fs::rename(&staged, dest).map_err(|e| {
        let _ = fs::remove_file(&staged);
        UpdateError::Install(e.to_string())
    })
}

#[cfg(target_os = "linux")]
fn apply_linux_tar_gz(
    archive: &Path,
    work_dir: &Path,
    dest_exe: &Path,
    update_desktop_files: bool,
) -> Result<ApplyOutcome, UpdateError> {
    let stage = work_dir.join("stage");
    let (new_binary, root) = extract_tar_gz_binary(archive, &stage)?;
    replace_linux_binary(&new_binary, dest_exe)?;

    if update_desktop_files && let Some(data) = dirs::data_dir() {
        let copies = [
            (
                root.join("gg.textures.app.desktop"),
                data.join("applications/gg.textures.app.desktop"),
            ),
            (
                root.join("gg.textures.app.svg"),
                data.join("icons/hicolor/scalable/apps/gg.textures.app.svg"),
            ),
            (
                root.join("gg.textures.app.png"),
                data.join("icons/hicolor/512x512/apps/gg.textures.app.png"),
            ),
        ];
        for (src, dst) in copies {
            if src.is_file() {
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&src, &dst);
            }
        }
    }

    let _ = fs::remove_dir_all(work_dir);
    std::process::Command::new(dest_exe)
        .spawn()
        .map_err(|e| UpdateError::Install(e.to_string()))?;
    Ok(ApplyOutcome::Relaunching)
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn safe_rel_path(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::tests::serve_once;

    #[test]
    fn only_a_later_three_number_version_is_newer() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("1.0.4", "1.0.4"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        for odd in ["", "2", "1.0", "1.0.0.0", "1.0.0-beta", "v9.9.9", "9.9.x"] {
            assert!(!newer(odd, "0.1.0"), "{odd}");
        }
    }

    #[test]
    fn the_check_reads_the_published_version_and_assets() {
        let payload = r#"{
            "version": "999.0.0",
            "released": "2026-11-27",
            "notes_url": "https://github.com/texturesgg/texturesgg/releases/tag/app-v999.0.0",
            "assets": {
                "windows": [
                    {"kind": "Installer", "url": "https://example.com/setup.exe", "size": 100, "sha256": "abc"},
                    {"kind": "Zip", "url": "https://example.com/app.zip", "size": 200, "sha256": "def"}
                ],
                "macos": [
                    {"kind": "dmg", "url": "https://example.com/app.dmg", "size": 300, "sha256": "ghi"}
                ],
                "linux": [
                    {"kind": "tar.gz", "url": "https://example.com/app.tar.gz", "size": 400, "sha256": "jkl"}
                ]
            }
        }"#;
        let (url, served) = serve_once("200 OK", payload);
        let release = check_at(&url, "0.1.0").expect("checked").expect("newer");
        served.join().expect("served");
        assert_eq!(release.version, "999.0.0");
        assert_eq!(
            release
                .asset_for(&InstallMethod::WindowsInstaller)
                .map(|a| a.kind.as_str()),
            Some("Installer")
        );
        assert_eq!(
            release
                .asset_for(&InstallMethod::WindowsZip)
                .map(|a| a.kind.as_str()),
            Some("Zip")
        );
        assert_eq!(
            release
                .asset_for(&InstallMethod::MacDmg)
                .map(|a| a.kind.as_str()),
            Some("dmg")
        );
        assert_eq!(
            release
                .asset_for(&InstallMethod::LinuxBinary(PathBuf::from(
                    "/tmp/tgg-studio"
                )))
                .map(|a| a.kind.as_str()),
            Some("tar.gz")
        );
        assert!(
            release
                .asset_for(&InstallMethod::Managed {
                    reason: "nix".into()
                })
                .is_none()
        );

        let (url, served) = serve_once("200 OK", r#"{"version":"0.0.1"}"#);
        assert_eq!(check_at(&url, "0.1.0").expect("checked"), None);
        served.join().expect("served");

        let (url, served) = serve_once("404 Not Found", "{}");
        assert!(matches!(check_at(&url, "0.1.0"), Err(NetError::Request(_))));
        served.join().expect("served");

        let (url, served) = serve_once("200 OK", "{}");
        assert!(matches!(
            check_at(&url, "0.1.0"),
            Err(NetError::Response(_))
        ));
        served.join().expect("served");
    }

    #[test]
    fn install_method_detection_handles_nix_system_and_user_paths() {
        assert!(matches!(
            detect_linux(
                Path::new("/nix/store/abc123-tgg-studio-1.0.3/bin/tgg-studio"),
                Some(Path::new("/home/user"))
            ),
            InstallMethod::Managed { .. }
        ));
        assert!(matches!(
            detect_linux(
                Path::new("/usr/bin/tgg-studio"),
                Some(Path::new("/home/user"))
            ),
            InstallMethod::Managed { .. }
        ));

        let temp = tempfile::tempdir().expect("tempdir");
        let local_bin = temp.path().join(".local/bin");
        fs::create_dir_all(&local_bin).expect("mkdir");
        let local_exe = local_bin.join("tgg-studio");
        assert_eq!(
            detect_linux(&local_exe, Some(temp.path())),
            InstallMethod::LinuxLocalInstall(local_exe)
        );
        let standalone = temp.path().join("tgg-studio");
        assert_eq!(
            detect_linux(&standalone, Some(temp.path())),
            InstallMethod::LinuxBinary(standalone)
        );

        // macOS bundle detection
        let app_dir = temp.path().join("textures.gg.app/Contents/MacOS");
        fs::create_dir_all(&app_dir).expect("mkdir");
        let mac_exe = app_dir.join("tgg-studio");
        assert_eq!(
            detect_macos(&mac_exe),
            InstallMethod::MacAppBundle(temp.path().join("textures.gg.app"))
        );

        // Windows installer vs portable zip detection
        let win_dir = temp.path().join("win-portable");
        fs::create_dir_all(&win_dir).expect("mkdir");
        assert_eq!(
            detect_windows(&win_dir.join("tgg-studio.exe")),
            InstallMethod::WindowsZip
        );
        fs::write(win_dir.join("unins000.exe"), b"").expect("unins");
        assert_eq!(
            detect_windows(&win_dir.join("tgg-studio.exe")),
            InstallMethod::WindowsInstaller
        );
    }

    #[test]
    fn download_verifies_sha256_and_rejects_mismatch() {
        let body = "hello updated binary";
        let expected_sha = format!("{:x}", Sha256::digest(body.as_bytes()));
        let temp = tempfile::tempdir().expect("tempdir");

        // Good checksum succeeds and reports progress.
        let (url, served) = serve_once("200 OK", body);
        let asset = Asset {
            kind: "tar.gz".into(),
            url: format!("{url}/textures.gg-1.0.5.tar.gz"),
            size: body.len() as u64,
            sha256: expected_sha,
            min_os: None,
        };
        let cancel = AtomicBool::new(false);
        let stages = std::sync::Mutex::new(Vec::new());
        let path = download_and_verify(
            &asset,
            temp.path(),
            &|p| stages.lock().expect("lock").push(p.stage),
            &cancel,
        )
        .expect("downloaded");
        served.join().expect("served");
        assert_eq!(fs::read_to_string(&path).expect("read"), body);
        let seen = stages.into_inner().expect("stages");
        assert!(seen.contains(&Stage::Downloading));
        assert!(seen.contains(&Stage::Verifying));

        // Bad checksum fails and removes partial file.
        let (url, served) = serve_once("200 OK", body);
        let bad = Asset {
            url: format!("{url}/bad.tar.gz"),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            ..asset
        };
        let err = download_and_verify(&bad, temp.path(), &|_| {}, &cancel).unwrap_err();
        served.join().expect("served");
        assert!(matches!(err, UpdateError::ChecksumMismatch { .. }));
        assert!(!temp.path().join("bad.tar.gz").exists());
        assert!(!temp.path().join("bad.tar.gz.part").exists());
    }

    #[test]
    fn tar_gz_extract_and_atomic_binary_replace_works() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive_path = temp.path().join("release.tar.gz");
        {
            let file = File::create(&archive_path).expect("create");
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            let data = b"new-binary-bytes";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(
                &mut header,
                "textures.gg-1.0.5-linux-x86_64/tgg-studio",
                &data[..],
            )
            .expect("append");
            tar.finish().expect("finish");
        }
        let stage = temp.path().join("stage");
        let (binary, _) = extract_tar_gz_binary(&archive_path, &stage).expect("extract");
        let dest = temp.path().join("tgg-studio");
        fs::write(&dest, b"old-binary").expect("old");
        replace_linux_binary(&binary, &dest).expect("replace");
        assert_eq!(fs::read(&dest).expect("read"), b"new-binary-bytes");
    }

    #[test]
    fn zip_extract_finds_windows_executable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let zip_path = temp.path().join("release.zip");
        {
            let file = File::create(&zip_path).expect("create");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("textures.gg-1.0.5-windows-x86_64/tgg-studio.exe", options)
                .expect("start_file");
            zip.write_all(b"mz-binary").expect("write");
            zip.finish().expect("finish");
        }
        let stage = temp.path().join("stage");
        let exe = extract_zip_binary(&zip_path, &stage).expect("extract");
        assert_eq!(fs::read(&exe).expect("read"), b"mz-binary");
    }
}
