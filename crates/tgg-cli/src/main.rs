//! `tgg`: the textures.gg command line.

mod mods;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tgg", version, about = "The textures.gg command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Code mods for tgg-mod-runtime: build, publish, install.
    #[command(subcommand)]
    Mod(mods::ModCommand),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Mod(command) => mods::run(command),
    }
}
