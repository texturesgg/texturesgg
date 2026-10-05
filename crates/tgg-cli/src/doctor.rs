//! `tgg doctor`: whether this machine can build and run mods, and the one
//! command that fixes what's missing.
//!
//! Mods build with GCC 12 or later: the game's headers use GCC's
//! `scalar_storage_order`, which clang lacks, and the options the SDK passes
//! need 12. The system's GCC is enough; the x86-64 ABI fixes the layout, and
//! the build's checks keep a mod loadable on the oldest glibc the game runs
//! on.

use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

/// The oldest GCC that builds mods.
const GCC_MIN: u32 = 12;
/// The oldest glibc the game runs on.
const GLIBC_MIN: (u32, u32) = (2, 34);

struct Report {
    problems: usize,
}

impl Report {
    fn ok(&self, what: &str, detail: &str) {
        println!("ok       {what}: {detail}");
    }

    fn note(&self, what: &str, detail: &str) {
        println!("note     {what}: {detail}");
    }

    fn problem(&mut self, what: &str, detail: &str) {
        self.problems += 1;
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

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
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

/// The command that installs a GCC new enough here.
fn install_gcc(nixpkgs: Option<&str>) -> String {
    let (id, like, version) = distro();
    let is = |name: &str| id == name || like.split(' ').any(|l| l == name);
    // The nixpkgs revision the game was built with gives the exact GCC the
    // game and the registry use.
    let nix = || {
        let rev = nixpkgs.unwrap_or("nixos-unstable");
        format!("nix shell github:NixOS/nixpkgs/{rev}#gcc15")
    };
    if id == "nixos" {
        return nix();
    }
    if id == "ubuntu" && version.starts_with("22.") {
        // 22.04's default gcc is 11.
        return "sudo apt install gcc-12, then export CC=gcc-12".into();
    }
    if is("debian") || is("ubuntu") {
        return "sudo apt install gcc".into();
    }
    if is("fedora") || is("rhel") {
        return "sudo dnf install gcc".into();
    }
    if id == "steamos" {
        return "install the SteamOS developer tools (sudo steamos-devmode enable), then sudo pacman -S gcc".into();
    }
    if is("arch") {
        return "sudo pacman -S gcc".into();
    }
    if is("suse") || is("opensuse") {
        return "sudo zypper install gcc".into();
    }
    if on_path("nix") {
        return nix();
    }
    "install GCC 12 or later with your system's package manager".into()
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
    let Some(version) = output(cc, &["-dumpfullversion"]) else {
        report.problem(
            "C compiler",
            &format!("{name} isn't there; {}", install_gcc(nixpkgs)),
        );
        return;
    };
    let banner = output(cc, &["--version"]).unwrap_or_default();
    if banner.contains("clang") {
        report.problem(
            "C compiler",
            &format!(
                "{name} is clang, which lacks the scalar_storage_order the game's headers use; {}",
                install_gcc(nixpkgs)
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
                None => install_gcc(nixpkgs),
            };
            report.problem(
                "C compiler",
                &format!("{name} is GCC {version}; mods need {GCC_MIN} or later: {fix}"),
            );
        }
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
    let mut report = Report { problems: 0 };
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        report.problem("system", "tgg-melee releases are for Linux x86-64 so far");
    } else {
        check_glibc(&mut report);
    }

    let mut nixpkgs = None;
    match crate::ports::current_version()? {
        None => report.problem("game", "tgg-melee isn't installed: tgg port install"),
        Some(_) => {
            let installed = crate::ports::resolve(None)?;
            match installed.release() {
                Ok(release) => {
                    report.ok(
                        "game",
                        &format!(
                            "tgg-melee {} (game layout {})",
                            release.version, release.game_abi
                        ),
                    );
                    let sdk = crate::paths::sdks()?.join(&release.game_abi);
                    if sdk.join(tgg_mod::sdk::FILE).is_file() {
                        report.ok("SDK", &sdk.display().to_string());
                    } else {
                        report.note("SDK", "not installed yet; the first build installs it");
                    }
                    nixpkgs = Some(release.toolchain.nixpkgs);
                }
                Err(error) => report.problem("game", &format!("{error:#}; tgg port install")),
            }
            if !installed.has_debug_info() {
                report.note(
                    "debug info",
                    &format!("for gdb: tgg port install {} --debug", installed.version),
                );
            }
        }
    }

    check_compiler(&mut report, cc, nixpkgs.as_deref());
    if !on_path("gdb") {
        report.note("gdb", "not found; only tgg mod dev --gdb needs it");
    }

    match crate::config::iso() {
        Ok(iso) => report.ok("disc image", &iso.display().to_string()),
        Err(error) => report.problem("disc image", &format!("{error:#}")),
    }
    report.ok(
        "mods folder",
        &tgg_mod::ModsDir::game().root().display().to_string(),
    );

    match report.problems {
        0 => Ok(()),
        1 => bail!("1 problem"),
        n => bail!("{n} problems"),
    }
}
