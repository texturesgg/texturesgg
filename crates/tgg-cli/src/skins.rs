//! `tgg skin`: the skins tgg.skins loads into tgg-melee (`skins.json`,
//! [`tgg_mod::skins`]).

use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};
use tgg_mod::Verdict;

#[derive(Subcommand)]
pub enum SkinCommand {
    /// Use a skin file (a .dat, such as PlMrNr.dat or one named PlMrNr-red.dat)
    /// in place of the game file it's for. tgg keeps a copy.
    Install {
        file: PathBuf,
        /// The game file it replaces [default: from its name: PlMrNr.dat for
        /// PlMrNr-red.dat]
        #[arg(long = "for", value_name = "GAME_FILE")]
        game_file: Option<String>,
        /// The name it shows [default: its file name]
        #[arg(long)]
        name: Option<String>,
    },
    /// List the skins tgg-melee loads.
    List,
    /// Put the game's own file back.
    Remove {
        /// The game file, such as PlMrNr.dat
        game_file: String,
    },
    /// Say whether a costume only changes looks, so it also shows in netplay
    /// games.
    Check {
        file: PathBuf,
        #[arg(long = "for", value_name = "GAME_FILE")]
        game_file: Option<String>,
    },
}

pub fn run(command: SkinCommand) -> Result<()> {
    let skins_file = tgg_mod::skins_file();
    match command {
        SkinCommand::Install {
            file,
            game_file,
            name,
        } => {
            let game_file = game_file_or_guess(game_file, &file)?;
            let bytes = std::fs::read(&file).with_context(|| file.display().to_string())?;
            // tgg's own copy, so the original can move
            let library = tgg_mod::default_data_dir().join("tgg").join("skins");
            let copy = library.join(format!("{}.dat", tgg_mod::package::sha256_hex(&bytes)));
            std::fs::create_dir_all(&library)
                .and_then(|()| std::fs::write(&copy, &bytes))
                .with_context(|| copy.display().to_string())?;
            let name = name.unwrap_or_else(|| stem(&file));
            tgg_mod::set_skin(&skins_file, &game_file, &copy, &name)?;
            println!("{name} replaces {game_file} from the next start");
            if !loader_installed() {
                println!(
                    "tgg.skins loads them: tgg mod install {}",
                    tgg_mod::skins::LOADER
                );
            }
            Ok(())
        }
        SkinCommand::List => {
            let skins = tgg_mod::skins(&skins_file)?;
            if skins.slots.is_empty() {
                println!("No skins (tgg skin install FILE)");
            }
            for (game_file, slot) in &skins.slots {
                println!("{game_file}  {}", slot.name);
            }
            Ok(())
        }
        SkinCommand::Remove { game_file } => {
            if tgg_mod::clear_skin(&skins_file, &game_file)? {
                println!("{game_file} is the game's own again from the next start");
            } else {
                println!("{game_file} has no skin");
            }
            Ok(())
        }
        SkinCommand::Check { file, game_file } => {
            let game_file = game_file_or_guess(game_file, &file)?;
            let port = tgg_mod::installed_ports(&crate::paths::ports()?)?
                .into_iter()
                .find(|port| port.current)
                .context("no tgg-melee installed: tgg port install")?;
            let iso = crate::config::iso()?;
            match tgg_mod::costume_verdict(&port, &iso, &game_file, &file)? {
                Verdict::LooksOnly => println!("Only changes looks: it shows in netplay games too"),
                Verdict::Counts(why) => println!(
                    "Changes more than looks ({why}): it shows offline, and netplay games show the normal costume"
                ),
                Verdict::NotACostume => println!("{game_file} isn't a costume"),
            }
            Ok(())
        }
    }
}

fn loader_installed() -> bool {
    tgg_mod::local(&tgg_mod::default_mods_dir(), None)
        .is_ok_and(|local| local.mods.iter().any(|m| m.id() == tgg_mod::skins::LOADER))
}

fn stem(file: &Path) -> String {
    file.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn game_file_or_guess(game_file: Option<String>, file: &Path) -> Result<String> {
    match game_file {
        Some(game_file) => Ok(game_file),
        None => game_file_of(file),
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
    anyhow::ensure!(
        tgg_mod::skins::is_slot(&game_file),
        "say which game file {name} replaces with --for"
    );
    Ok(game_file)
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
        assert_eq!(game_file_of(Path::new("GrNBa.dat")).unwrap(), "GrNBa.dat");
    }
}
