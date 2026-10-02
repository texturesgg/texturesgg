//! Native entry for the editor shell.
//!
//! ```text
//! tgg-editor [COSTUME] [--iso PATH] [--exit-after SECONDS]
//! tgg-editor --stress-test [--iso PATH] [--exit-after SECONDS]
//! ```
//!
//! Without COSTUME the app opens on the player's costumes, after a welcome
//! screen that finds their Melee ISO (or asks for it) the first time.
//! COSTUME opens straight into the editor: a DAT on disk, or the name of one
//! in the player's game (`PlFcRe.dat`). The game is the ISO given with
//! `--iso`, else the one chosen before or found through Slippi; a stock
//! costume then plays its fighter's animations and names its textures.
//! `--stress-test` opens Settings' stress test, every
//! costume in the game animated at once. `--exit-after` quits on a timer,
//! for measuring headless runs.

use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;
use tgg_editor::{
    Costume, Launch, Start, find_game, game_references, load_model, remember_game, run, saved_game,
    start_log,
};

const USAGE: &str = "usage: tgg-editor [COSTUME] [--iso PATH] [--exit-after SECONDS]
       tgg-editor --stress-test [--iso PATH] [--exit-after SECONDS]";

struct Args {
    dat: Option<PathBuf>,
    iso: Option<PathBuf>,
    exit_after: Option<Duration>,
    stress_test: bool,
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut dat = None;
    let mut iso = None;
    let mut exit_after = None;
    let mut stress_test = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--stress-test" => stress_test = true,
            "--iso" => iso = Some(args.next().ok_or("--iso needs a PATH")?.into()),
            "--exit-after" => {
                let seconds: f64 = args
                    .next()
                    .ok_or("--exit-after needs SECONDS")?
                    .parse()
                    .map_err(|error| format!("--exit-after: {error}"))?;
                exit_after = Some(Duration::from_secs_f64(seconds));
            }
            _ if dat.is_none() && !argument.starts_with("--") => dat = Some(argument.into()),
            _ => return Err(format!("unexpected argument {argument}\n{USAGE}").into()),
        }
    }
    if stress_test && dat.is_some() {
        return Err(format!("--stress-test runs on the game alone\n{USAGE}").into());
    }
    Ok(Args {
        dat,
        iso,
        exit_after,
        stress_test,
    })
}

fn launch(args: Args) -> Result<Launch, Box<dyn Error>> {
    let Some(dat) = args.dat else {
        // Home: the game given, else the one chosen before; the welcome
        // screen finds one otherwise, or says why a given one won't do.
        let (game, problem) = match (args.iso.as_deref(), saved_game()) {
            (Some(iso), _) => match find_game(Some(iso)) {
                Ok(game) => (game.map(|(game, _)| game), None),
                Err(problem) => (None, Some(problem.to_string())),
            },
            (None, saved) => (saved, None),
        };
        if let Some(game) = &game {
            remember_game(game);
        }
        return Ok(Launch {
            start: if args.stress_test && problem.is_none() {
                Start::StressTest
            } else {
                Start::Home { problem }
            },
            references: game.map(game_references).transpose()?,
            exit_after: args.exit_after,
        });
    };
    let references = find_game(args.iso.as_deref())?
        .map(|(game, _)| game_references(game))
        .transpose()?;
    if let Some(references) = &references {
        remember_game(references.game());
    }
    Ok(Launch {
        start: Start::Costume(read_costume(dat, references.as_ref())?),
        references,
        exit_after: args.exit_after,
    })
}

/// A costume on disk, or one in the player's game when none is on disk.
/// Without references it shows its bind pose.
fn read_costume(
    dat: PathBuf,
    references: Option<&tgg_editor::References>,
) -> Result<Costume, Box<dyn Error>> {
    let (bytes, path) = match std::fs::read(&dat) {
        Ok(bytes) => (bytes, Some(dat.clone())),
        Err(error) => {
            let in_game = references
                .zip(dat.to_str())
                .map(|(references, name)| references.game().read(name));
            match in_game {
                Some(Ok(bytes)) => (bytes, None),
                _ => return Err(format!("{}: {error}", dat.display()).into()),
            }
        }
    };
    let name = file_name(&dat);
    let loaded = load_model(&name, &bytes, references)?;
    Ok(Costume {
        loaded,
        name,
        dat: bytes,
        path,
    })
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn main() {
    start_log();
    match parse_args().and_then(launch) {
        Ok(launch) => run(launch),
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
    }
}
