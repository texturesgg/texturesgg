//! `tgg`: the textures.gg command line.

mod account;
mod build;
mod config;
mod dev;
mod docs;
mod doctor;
mod mods;
mod paths;
mod ports;
mod publish;
mod releases;
mod scaffold;
mod sdks;

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
    /// Install, pick and run released builds of tgg-melee.
    #[command(subcommand)]
    Port(ports::PortCommand),
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Login { no_browser } => account::login(&cli.api, !no_browser),
        Command::Logout => account::logout(&cli.api),
        Command::Port(command) => ports::run(command),
        Command::Sdk(command) => sdks::run(command),
        Command::Mod(command) => mods::run(command, &cli.api),
        Command::Config(command) => config::run(command),
        Command::Doctor { cc } => doctor::run(&cc.cc),
        Command::Docs { page } => docs::run(page.as_deref()),
    }
}
