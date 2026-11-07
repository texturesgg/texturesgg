//! `tgg doctor`: whether this machine can play tgg-melee and make mods for
//! it, and the one command that fixes each thing missing.
//!
//! Playing needs the game, what it opens at runtime (on Linux glibc 2.34 or
//! later, Vulkan and X11 or Wayland; on Windows the Microsoft C++ runtime)
//! and a disc image. Making mods needs GCC 12 or later: the game's headers
//! use GCC's `scalar_storage_order`, which clang lacks, and the options the
//! SDK passes need 12. On Windows that is a MinGW GCC linking msvcrt, as the
//! game does; on macOS Homebrew's. The SDK's own CMake files build each mod
//! (TggMod.cmake), so CMake 3.25 or later is needed too, and Ninja (required
//! on Windows), and git keeps a mod's history.

use anyhow::{Result, bail};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::paths::{find_program, on_path};
use crate::toolchain::HOMEBREW_BIN;

/// The oldest GCC that builds mods.
const GCC_MIN: u32 = 12;
/// The oldest glibc the game runs on.
const GLIBC_MIN: (u32, u32) = (2, 34);
/// The oldest CMake the SDK's TggMod.cmake takes.
const CMAKE_MIN: (u32, u32) = (3, 25);

#[derive(Default)]
struct Report {
    /// Problems for playing, and for making mods.
    play: usize,
    make: usize,
    making: bool,
}

impl Report {
    fn ok(&self, what: &str, detail: &str) {
        println!("ok       {what}: {detail}");
    }

    fn note(&self, what: &str, detail: &str) {
        println!("note     {what}: {detail}");
    }

    fn problem(&mut self, what: &str, detail: &str) {
        if self.making {
            self.make += 1;
        } else {
            self.play += 1;
        }
        println!("problem  {what}: {detail}");
    }
}

/// The output of `program args`, if it runs and succeeds.
fn output(program: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The output of `program args` given `input`, if it runs and succeeds.
fn output_with(program: &Path, args: &[&str], input: &str) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The system's distribution, from /etc/os-release: its `ID`, `ID_LIKE` and
/// `VERSION_ID`.
fn distro() -> (String, String, String) {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned()
    };
    (field("ID"), field("ID_LIKE"), field("VERSION_ID"))
}

/// A tool making mods needs.
#[derive(Clone, Copy)]
enum Tool {
    Gcc,
    Cmake,
    Ninja,
    Git,
}

impl Tool {
    /// Its package on Debian-like, Fedora-like, Arch-like and SUSE systems,
    /// and in nixpkgs.
    fn packages(self) -> [&'static str; 5] {
        match self {
            Tool::Gcc => ["gcc", "gcc", "gcc", "gcc", "gcc15"],
            Tool::Cmake => ["cmake", "cmake", "cmake", "cmake", "cmake"],
            Tool::Ninja => ["ninja-build", "ninja-build", "ninja", "ninja", "ninja"],
            Tool::Git => ["git", "git", "git", "git", "git"],
        }
    }

    /// Its winget package: GCC is WinLibs' build linking msvcrt, as the game
    /// does.
    fn winget(self) -> &'static str {
        match self {
            Tool::Gcc => "BrechtSanders.WinLibs.POSIX.MSVCRT",
            Tool::Cmake => "Kitware.CMake",
            Tool::Ninja => "Ninja-build.Ninja",
            Tool::Git => "Git.Git",
        }
    }

    /// Its Homebrew formula: GCC 15 is the one the SDK's toolchain file looks
    /// for first.
    fn brew(self) -> &'static str {
        match self {
            Tool::Gcc => "gcc@15",
            Tool::Cmake => "cmake",
            Tool::Ninja => "ninja",
            Tool::Git => "git",
        }
    }
}

fn has_homebrew() -> bool {
    on_path("brew")
        || HOMEBREW_BIN
            .iter()
            .any(|dir| Path::new(dir).join("brew").is_file())
}

/// The command that installs `tool`, new enough, here.
fn install(tool: Tool, nixpkgs: Option<&str>) -> String {
    if cfg!(windows) {
        return format!("winget install {}, then open a new terminal", tool.winget());
    }
    if cfg!(target_os = "macos") {
        if matches!(tool, Tool::Git) {
            return "xcode-select --install".into();
        }
        return if has_homebrew() {
            format!("brew install {}", tool.brew())
        } else {
            format!(
                "install Homebrew (https://brew.sh), then brew install {}",
                tool.brew()
            )
        };
    }
    let (id, like, version) = distro();
    let is = |name: &str| id == name || like.split(' ').any(|l| l == name);
    let [apt, dnf, pacman, zypper, nix_name] = tool.packages();
    // The nixpkgs revision the game was built with gives the exact GCC the
    // game and the registry use.
    let nix = || {
        let rev = nixpkgs.unwrap_or("nixos-unstable");
        format!("nix shell github:NixOS/nixpkgs/{rev}#{nix_name}")
    };
    if id == "nixos" {
        return nix();
    }
    if id == "ubuntu" && version.starts_with("22.") {
        // 22.04's default gcc is 11 and its cmake 3.22.
        match tool {
            Tool::Gcc => return "sudo apt install gcc-12, then export CC=gcc-12".into(),
            Tool::Cmake => return "sudo snap install cmake --classic".into(),
            Tool::Ninja | Tool::Git => {}
        }
    }
    if is("debian") || is("ubuntu") {
        return format!("sudo apt install {apt}");
    }
    if is("fedora") || is("rhel") {
        return format!("sudo dnf install {dnf}");
    }
    if id == "steamos" {
        return format!(
            "install the SteamOS developer tools (sudo steamos-devmode enable), then sudo pacman -S {pacman}"
        );
    }
    if is("arch") {
        return format!("sudo pacman -S {pacman}");
    }
    if is("suse") || is("opensuse") {
        return format!("sudo zypper install {zypper}");
    }
    if on_path("nix") {
        return nix();
    }
    format!("install {apt} with your system's package manager")
}

fn check_build_tools(report: &mut Report, nixpkgs: Option<&str>) {
    match output(Path::new("cmake"), &["--version"]) {
        None => report.problem(
            "CMake",
            &format!(
                "not found; mods build with the SDK's CMake files: {}",
                install(Tool::Cmake, nixpkgs)
            ),
        ),
        Some(text) => {
            let version = text
                .lines()
                .next()
                .unwrap_or_default()
                .trim_start_matches("cmake version ")
                .to_owned();
            let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
            let have = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
            if have >= CMAKE_MIN {
                report.ok("CMake", &version);
            } else {
                report.problem(
                    "CMake",
                    &format!(
                        "{version}; mods need {}.{} or later: {}",
                        CMAKE_MIN.0,
                        CMAKE_MIN.1,
                        install(Tool::Cmake, nixpkgs)
                    ),
                );
            }
        }
    }
    match output(Path::new("ninja"), &["--version"]) {
        Some(version) => report.ok("Ninja", &version),
        None if cfg!(windows) => report.problem(
            "Ninja",
            &format!(
                "not found; mods build with it on Windows: {}",
                install(Tool::Ninja, nixpkgs)
            ),
        ),
        None => report.note(
            "Ninja",
            &format!(
                "not found, so builds use make, which is slower: {}",
                install(Tool::Ninja, nixpkgs)
            ),
        ),
    }
    let git = Path::new("git");
    match output(git, &["--version"]) {
        None => report.problem(
            "git",
            &format!(
                "not found; a mod's history and publishing need it: {}",
                install(Tool::Git, nixpkgs)
            ),
        ),
        Some(version) => {
            let who = ["user.name", "user.email"]
                .iter()
                .all(|key| output(git, &["config", key]).is_some_and(|v| !v.is_empty()));
            if who {
                report.ok("git", version.trim_start_matches("git version "));
            } else {
                report.note(
                    "git",
                    "it doesn't know who you are, so it can't commit: \
                     git config --global user.name \"Your Name\"; \
                     git config --global user.email you@example.com",
                );
            }
        }
    }
}

fn major(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

/// A newer `gcc-<n>` on PATH, for when `gcc` itself is too old.
fn newer_gcc() -> Option<String> {
    (GCC_MIN..=20)
        .rev()
        .map(|n| format!("gcc-{n}"))
        .find(|name| on_path(name))
}

fn check_compiler(report: &mut Report, cc: &Path, nixpkgs: Option<&str>) {
    let name = cc.display().to_string();
    let banner = output(cc, &["--version"]).unwrap_or_default();
    if banner.contains("clang") {
        let what = if cfg!(target_os = "macos") && name == "gcc" {
            "gcc is Apple's clang".to_owned()
        } else {
            format!("{name} is clang")
        };
        report.problem(
            "C compiler",
            &format!(
                "{what}, which lacks the scalar_storage_order the game's headers use; {}",
                install(Tool::Gcc, nixpkgs)
            ),
        );
        return;
    }
    let Some(version) = output(cc, &["-dumpfullversion"]) else {
        report.problem(
            "C compiler",
            &format!("{name} isn't there; {}", install(Tool::Gcc, nixpkgs)),
        );
        return;
    };
    // A MinGW GCC for the UCRT builds mods that link another C runtime than
    // the game's msvcrt.
    if cfg!(windows)
        && output_with(cc, &["-E", "-dM", "-x", "c", "-"], "#include <stdio.h>\n")
            .is_some_and(|defines| defines.contains("#define _UCRT"))
    {
        report.problem(
            "C compiler",
            &format!(
                "{name} is a UCRT GCC; mods link msvcrt, as the game does: {}, and put it \
                 first on PATH",
                install(Tool::Gcc, nixpkgs)
            ),
        );
        return;
    }
    match major(&version) {
        Some(major) if major >= GCC_MIN => {
            report.ok("C compiler", &format!("GCC {version} ({name})"))
        }
        _ => {
            let fix = match newer_gcc() {
                Some(newer) => format!("export CC={newer}"),
                None => install(Tool::Gcc, nixpkgs),
            };
            report.problem(
                "C compiler",
                &format!("{name} is GCC {version}; mods need {GCC_MIN} or later: {fix}"),
            );
        }
    }
}

/// macOS: the SDK GCC compiles against comes with Xcode's command line
/// tools, and Homebrew's programs need its folder on PATH.
fn check_mac(report: &mut Report) {
    match output(Path::new("xcrun"), &["--show-sdk-path"]) {
        Some(sdk) if !sdk.is_empty() => report.ok("macOS SDK", &sdk),
        _ => report.problem(
            "macOS SDK",
            "Xcode's command line tools aren't installed: xcode-select --install",
        ),
    }
    if let Some(dir) = HOMEBREW_BIN
        .iter()
        .find(|dir| Path::new(dir).join("brew").is_file())
        && find_program("brew").is_none()
    {
        report.problem(
            "Homebrew",
            &format!(
                "{dir} isn't on PATH: add eval \"$({dir}/brew shellenv)\" to your shell's profile"
            ),
        );
    }
}

fn check_glibc(report: &mut Report) {
    let Some(text) = output(Path::new("getconf"), &["GNU_LIBC_VERSION"]) else {
        report.problem(
            "glibc",
            "this system's C library isn't glibc; the game needs glibc 2.34 or later",
        );
        return;
    };
    let version = text.trim_start_matches("glibc ").to_owned();
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let have = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if have >= GLIBC_MIN {
        report.ok("glibc", &version);
    } else {
        report.problem(
            "glibc",
            &format!(
                "{version}; the game needs {}.{} or later",
                GLIBC_MIN.0, GLIBC_MIN.1
            ),
        );
    }
}

pub fn run(cc: &Path) -> Result<()> {
    let mut report = Report::default();
    println!("Playing");
    match crate::releases::target() {
        None => report.problem(
            "system",
            &format!(
                "tgg-melee is built for Linux on x86-64, Windows on x86-64 and macOS on Apple \
                 silicon; this is {}",
                crate::releases::system()
            ),
        ),
        Some(_) => report.ok("system", &crate::releases::system()),
    }
    if cfg!(target_os = "linux") {
        check_glibc(&mut report);
        check_graphics(&mut report);
    }

    let mut nixpkgs = None;
    let mut installed = None;
    match crate::ports::current_version()? {
        None => report.problem("game", "tgg-melee isn't installed: tgg port install"),
        Some(_) => {
            let port = crate::ports::resolve(None)?;
            match port.release() {
                Ok(release) => {
                    report.ok(
                        "game",
                        &format!(
                            "tgg-melee {} (game layout {})",
                            release.version, release.game_abi
                        ),
                    );
                    nixpkgs = Some(release.toolchain.nixpkgs.clone());
                    installed = Some((port, release));
                }
                Err(error) => report.problem("game", &format!("{error:#}; tgg port install")),
            }
        }
    }
    if cfg!(windows) {
        let folder = installed.as_ref().map(|(port, _)| port.folder.as_path());
        let missing = crate::ports::missing_runtime(folder);
        if missing.is_empty() {
            report.ok("C++ runtime", "installed");
        } else {
            report.problem(
                "C++ runtime",
                &format!(
                    "the game needs Microsoft's Visual C++ runtime ({} missing): {}",
                    missing.join(", "),
                    crate::ports::VC_RUNTIME_FIX
                ),
            );
        }
    }
    match crate::config::iso() {
        Ok(iso) => report.ok("disc image", &iso.display().to_string()),
        Err(error) => report.problem("disc image", &format!("{error:#}")),
    }
    report.ok(
        "mods folder",
        &tgg_mod::ModsDir::game().root().display().to_string(),
    );

    println!("\nMaking mods");
    report.making = true;
    if let Some((port, release)) = &installed {
        let sdk = crate::paths::sdks()?.join(&release.game_abi);
        if sdk.join(tgg_mod::sdk::FILE).is_file() {
            report.ok("SDK", &sdk.display().to_string());
        } else {
            report.note("SDK", "not installed yet; the first build installs it");
        }
        if release.files.debug.is_some() && !port.has_debug_info() {
            report.note(
                "debug info",
                &format!("for gdb: tgg port install {} --debug", port.version),
            );
        }
    }
    if cfg!(target_os = "macos") {
        check_mac(&mut report);
    }
    check_compiler(&mut report, cc, nixpkgs.as_deref());
    check_build_tools(&mut report, nixpkgs.as_deref());
    if cfg!(target_os = "linux") && !on_path("gdb") {
        report.note("gdb", "not found; only tgg mod dev --gdb needs it");
    }

    let plural = |n: usize| {
        if n == 1 {
            "1 problem".to_owned()
        } else {
            format!("{n} problems")
        }
    };
    println!();
    match (report.play, report.make) {
        (0, 0) => {
            println!("All set");
            Ok(())
        }
        (0, make) => bail!("ready to play; {} for making mods", plural(make)),
        (play, make) => bail!("{}", plural(play + make)),
    }
}

/// Folders the system's dynamic loader searches, as far as tgg can tell
/// without loading anything: `LD_LIBRARY_PATH`, nix-ld's libraries,
/// `ldconfig`'s cache and the usual folders.
fn library_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = ["LD_LIBRARY_PATH", "NIX_LD_LIBRARY_PATH"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .collect();
    for ldconfig in ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"] {
        if let Some(cache) = output(Path::new(ldconfig), &["-p"]) {
            dirs.extend(cache.lines().filter_map(|line| {
                let path = Path::new(line.rsplit_once("=> ")?.1.trim());
                Some(path.parent()?.to_owned())
            }));
            break;
        }
    }
    dirs.extend(
        [
            "/lib64",
            "/usr/lib64",
            "/lib/x86_64-linux-gnu",
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib",
            "/lib",
        ]
        .map(std::path::PathBuf::from),
    );
    dirs
}

fn has_library(dirs: &[std::path::PathBuf], name: &str) -> bool {
    dirs.iter().any(|dir| dir.join(name).exists())
}

/// The game opens Vulkan and an X11 or Wayland client library at runtime.
fn check_graphics(report: &mut Report) {
    let dirs = library_dirs();
    let (id, like, _) = distro();
    let is = |name: &str| id == name || like.split(' ').any(|l| l == name);
    let install = |apt: &str, dnf: &str, pacman: &str, nix: &str| {
        if id == "nixos" {
            format!("add {nix} to programs.nix-ld.libraries in your NixOS configuration")
        } else if is("debian") || is("ubuntu") {
            format!("sudo apt install {apt}")
        } else if is("fedora") || is("rhel") {
            format!("sudo dnf install {dnf}")
        } else if is("arch") || id == "steamos" {
            format!("sudo pacman -S {pacman}")
        } else {
            format!("install {apt} with your system's package manager")
        }
    };
    if has_library(&dirs, "libvulkan.so.1") {
        report.ok("Vulkan", "libvulkan.so.1");
    } else {
        report.problem(
            "Vulkan",
            &format!(
                "the game draws with Vulkan and can't find libvulkan.so.1: {}",
                install(
                    "libvulkan1",
                    "vulkan-loader",
                    "vulkan-icd-loader",
                    "vulkan-loader"
                )
            ),
        );
    }
    let icds = std::env::var_os("VK_ICD_FILENAMES").is_some()
        || [
            "/usr/share/vulkan/icd.d",
            "/etc/vulkan/icd.d",
            "/run/opengl-driver/share/vulkan/icd.d",
        ]
        .iter()
        .any(|dir| std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some()));
    if !icds {
        report.note(
            "Vulkan",
            "no Vulkan driver found; install your GPU's (Mesa's for AMD and Intel), or run \
             the game on the CPU with TGG_CPU_GPU=1 and Mesa's lavapipe",
        );
    }
    let x11 = [
        "libX11.so.6",
        "libXext.so.6",
        "libXcursor.so.1",
        "libXi.so.6",
        "libXrandr.so.2",
    ];
    let has_x11 = x11.iter().all(|lib| has_library(&dirs, lib));
    let has_wayland =
        has_library(&dirs, "libwayland-client.so.0") && has_library(&dirs, "libxkbcommon.so.0");
    match (has_x11, has_wayland) {
        (true, true) => report.ok("window", "X11 and Wayland"),
        (true, false) => report.ok("window", "X11"),
        (false, true) => report.ok(
            "window",
            "Wayland (X11's libraries are missing, so the game can't open under X11 or Xvfb)",
        ),
        (false, false) => report.problem(
            "window",
            &format!(
                "the game opens its window with X11 or Wayland and can't find either's libraries: {}",
                install(
                    "libx11-6 libxext6 libxcursor1 libxi6 libxrandr2 libxkbcommon0",
                    "libX11 libXext libXcursor libXi libXrandr libxkbcommon",
                    "libx11 libxext libxcursor libxi libxrandr libxkbcommon",
                    "libX11, libXext, libXcursor, libXi, libXrandr and libxkbcommon"
                )
            ),
        ),
    }
}
