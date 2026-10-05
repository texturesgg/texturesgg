//! `tgg`: the textures.gg command line.

mod account;
mod mods;

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
    /// Code mods for tgg-melee: build, publish, install.
    #[command(subcommand)]
    Mod(mods::ModCommand),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Login { no_browser } => account::login(&cli.api, !no_browser),
        Command::Logout => account::logout(&cli.api),
        Command::Mod(command) => mods::run(command, &cli.api),
    }
}
