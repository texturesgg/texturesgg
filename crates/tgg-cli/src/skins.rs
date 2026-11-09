//! `tgg skin`: skins in tgg-melee, each a mod of one game file in the game's
//! mods folder ([`tgg_mod::skins`]).

use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};
use tgg_mod::{Netplay, Skin};

#[derive(Subcommand)]
pub enum SkinCommand {
    /// Install a skin file (a .dat, such as PlMrNr.dat or one named
    /// PlMrNr-red.dat) in place of the game file it's for.
    Install {
        file: PathBuf,
        /// The game file it replaces [default: from its name: PlMrNr.dat
        /// for PlMrNr-red.dat]
        #[arg(long = "for", value_name = "GAME_FILE")]
        game_file: Option<String>,
        /// The name it shows [default: its file name]
        #[arg(long)]
        name: Option<String>,
    },
    /// List the skins installed in tgg-melee.
    List,
    /// Put the game's own file back.
    Remove {
        /// The game file, such as PlMrNr.dat
        game_file: String,
    },
}

pub fn run(command: SkinCommand) -> Result<()> {
    let mods = tgg_mod::default_mods_dir();
    match command {
        SkinCommand::Install {
            file,
            game_file,
            name,
        } => {
            let game_file = match game_file {
                Some(game_file) => game_file,
                None => game_file_of(&file)?,
            };
            let bytes = std::fs::read(&file).with_context(|| file.display().to_string())?;
            let name = name.unwrap_or_else(|| {
                file.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            let skin = tgg_mod::install_skin(&mods, &game_file, &name, bytes)?;
            println!(
                "Installed {} for {}{}",
                skin.name,
                skin.path,
                netplay_note(&skin)
            );
            if !skin.enabled {
                println!("It's off: tgg mod on {}", skin.id);
            }
            Ok(())
        }
        SkinCommand::List => {
            let skins = tgg_mod::skins(&tgg_mod::local(&mods, None)?);
            if skins.is_empty() {
                println!("No skins in {}", crate::ui::path(mods.display()));
            }
            for skin in skins {
                let off = if skin.enabled { "" } else { " (off)" };
                println!("{}  {}{off}{}", skin.path, skin.name, netplay_note(&skin));
            }
            Ok(())
        }
        SkinCommand::Remove { game_file } => {
            tgg_mod::uninstall_skin(&mods, &game_file)?;
            println!("{game_file} is the game's own again");
            Ok(())
        }
    }
}

/// The game file a skin file is for, from its name: `PlMrNr.dat` for
/// `PlMrNr-red.dat` or `PlMrNr_red.dat`.
fn game_file_of(file: &Path) -> Result<String> {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, extension) = name.rsplit_once('.').unwrap_or((&name, "dat"));
    let stem = stem.split(['-', '_', ' ']).next().unwrap_or(stem);
    let game_file = format!("{stem}.{extension}");
    tgg_mod::skin_id(&game_file)
        .map(|_| game_file)
        .with_context(|| format!("say which game file {name} replaces with --for"))
}

fn netplay_note(skin: &Skin) -> &'static str {
    if skin.netplay == Netplay::Costumes {
        ""
    } else {
        "; players you meet online need the same file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skins_game_file_comes_from_its_name() {
        assert_eq!(
            game_file_of(Path::new("a/PlMrNr-red.dat")).unwrap(),
            "PlMrNr.dat"
        );
        assert_eq!(
            game_file_of(Path::new("PlFxOr_DARK.dat")).unwrap(),
            "PlFxOr.dat"
        );
        assert_eq!(game_file_of(Path::new("GrNBa.dat")).unwrap(), "GrNBa.dat");
    }
}
