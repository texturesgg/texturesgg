//! `tgg`: the textures.gg command line.

mod account;
mod build;
mod config;
mod defaults;
mod dev;
mod docs;
mod doctor;
mod mods;
mod paths;
mod player;
mod ports;
mod profiles;
mod publish;
mod releases;
mod scaffold;
mod sdks;
mod signals;
mod skins;
mod toolchain;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tgg", version, about = "The textures.gg command line")]
struct Cli {
    /// The textures.gg API to talk to.
    #[arg(
        long,
        env = "TGG_API",
        default_value = "https://api.textures.gg",
        global = true
    )]
    api: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in to textures.gg by confirming a code in your browser.
    Login {
        /// Print the link instead of opening a browser.
        #[arg(long)]
        no_browser: bool,
    },
    /// Sign out of textures.gg.
    Logout,
    /// Your textures.gg account: tickets for its services.
    #[command(subcommand)]
    Account(account::AccountCommand),
    /// Install, pick and run released builds of tgg-melee.
    #[command(subcommand)]
    Port(ports::PortCommand),
    /// Play a replay (.slp) with exactly the game and mods it was played
    /// with, fetched for this session; your own mods stay as they are.
    Replay {
        file: std::path::PathBuf,
        /// Don't ask: fetch every mod it needs and play even if some can't
        /// be had.
        #[arg(short, long)]
        yes: bool,
        /// Show the game's own output as it runs.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Mod profiles: the exact mods a replay or a profile file needs.
    #[command(subcommand)]
    Profile(profiles::ProfileCommand),
    /// Skins in tgg-melee: a costume or menu file in place of the game's.
    #[command(subcommand)]
    Skin(skins::SkinCommand),
    /// The SDK mods build against.
    #[command(subcommand)]
    Sdk(sdks::SdkCommand),
    /// Code mods for tgg-melee: build, publish, install.
    #[command(subcommand)]
    Mod(mods::ModCommand),
    /// Open the mod docs for the installed game (offline, the SDK's copy).
    Docs {
        /// A page, such as writing-mods or hooks [default: the index]
        page: Option<String>,
    },
    /// Check this machine can build and run mods, and say how to fix what's
    /// missing.
    Doctor {
        #[command(flatten)]
        cc: mods::Cc,
    },
    /// Settings, such as your disc image.
    #[command(subcommand)]
    Config(config::ConfigCommand),
}

/// Errors print as their message (and causes, but for tgg-mod's), never a
/// backtrace, whatever `RUST_BACKTRACE` says; `TGG_BACKTRACE=1` adds it.
fn main() -> std::process::ExitCode {
    ui::init();
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // tgg-mod's errors read whole on their own; their sources (such
            // as why textures.gg can't be reached) are detail.
            if error.downcast_ref::<tgg_mod::Error>().is_some() {
                eprintln!("Error: {error}");
            } else {
                eprintln!("Error: {error:#}");
            }
            if std::env::var_os("TGG_BACKTRACE").is_some_and(|value| value == "1") {
                eprintln!("\n{}", error.backtrace());
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Login { no_browser } => account::login(&cli.api, !no_browser),
        Command::Logout => account::logout(&cli.api),
        Command::Account(command) => account::run(command, &cli.api),
        Command::Port(command) => ports::run(command, &cli.api),
        Command::Replay { file, yes, verbose } => {
            profiles::play(&cli.api, &file, Some(&file), yes, verbose)
        }
        Command::Profile(command) => profiles::run(command, &cli.api),
        Command::Skin(command) => skins::run(command),
        Command::Sdk(command) => sdks::run(command),
        Command::Mod(command) => mods::run(command, &cli.api),
        Command::Config(command) => config::run(command),
        Command::Doctor { cc } => doctor::run(&cc.resolve()),
        Command::Docs { page } => docs::run(page.as_deref()),
    }
}
