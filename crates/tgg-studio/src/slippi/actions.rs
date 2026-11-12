//! Finding Slippi's Dolphin and playing the player's ISO in it.

use crate::jobs::Done as Job;
use crate::state::App;

/// Look for Slippi's netplay Dolphin.
pub fn find(app: &mut App) {
    app.slippi.dolphin = tgg_app::slippi::dolphin();
}

/// Open the player's ISO, skins and all, in Slippi's Dolphin, and note when
/// it closes.
pub fn play(app: &mut App) {
    if app.slippi.running {
        return;
    }
    let (Some(dolphin), Some(references)) = (&app.slippi.dolphin, &app.game) else {
        return;
    };
    match tgg_app::slippi::launch(dolphin, references.game().path()) {
        Ok(mut child) => {
            app.slippi.running = true;
            app.tell("Slippi is opening.", false);
            app.jobs.spawn("slippi", move || {
                let _ = child.wait();
                Job::SlippiEnded
            });
        }
        Err(error) => app.tell(format!("Slippi couldn't start: {error}."), true),
    }
}

/// Dolphin closed.
pub fn ended(app: &mut App) -> bool {
    app.slippi.running = false;
    true
}
