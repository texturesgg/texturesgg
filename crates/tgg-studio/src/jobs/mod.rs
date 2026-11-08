//! Background work, so the window never waits on the disc: reading what
//! every slot of the player's game holds, and rendering costumes, each on a
//! thread of its own. Results come back down one channel and wake the
//! window; `poll` hands them to the actions that own them before a frame.
//! Renders skip the channel: the worker fills each picture itself.

pub mod renders;

use melee_dat::MeleeSlot;
use rune::Waker;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tgg_app::install::{History, SlotState};
use tgg_app::library::Library;

/// What a finished job found.
pub enum Done {
    /// What each slot of the game at `iso` holds, and which have an install
    /// to undo.
    States {
        iso: PathBuf,
        states: HashMap<MeleeSlot, SlotState>,
        undoable: HashSet<MeleeSlot>,
    },
    /// Mods' work: what textures.gg offers, an install, the game ending.
    Mods(Box<crate::mods::Done>),
    /// A report was sent, under this id, or why not.
    ReportSent(Result<String, String>),
    /// A newer version of the app, if the launch check found one.
    Update(Option<String>),
}

pub struct Jobs {
    /// Wakes the window when a result arrives; `None` until the app is
    /// running (and in tests, which take results themselves).
    waker: Option<Waker>,
    done: mpsc::Sender<Done>,
    results: mpsc::Receiver<Done>,
    /// Run spawned work on the window's thread, to its end, before the
    /// next frame: for recordings, whose clock runs ahead of real time.
    pub inline: bool,
}

impl Default for Jobs {
    fn default() -> Self {
        let (done, results) = mpsc::channel();
        Self {
            waker: None,
            done,
            results,
            inline: false,
        }
    }
}

impl Jobs {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Read what every slot of the game at `iso` holds, against the
    /// library at `library`: each slot's file, compared with vanilla and the
    /// library's skins; and which slots the install history at `history` can
    /// undo.
    pub fn read_states(&self, iso: &Path, library: &Path, history: &Path) {
        let (iso, library, history) = (iso.to_owned(), library.to_owned(), history.to_owned());
        let (done, waker) = (self.done.clone(), self.waker.clone());
        std::thread::Builder::new()
            .name("slot states".into())
            .spawn(move || {
                let (states, undoable) = match tgg_app::Game::open(&iso) {
                    Ok(game) => {
                        let history = History::open_at(history);
                        let undoable = game
                            .slots()
                            .into_iter()
                            .filter(|slot| history.can_undo(&game, *slot))
                            .collect();
                        (states_of(&game, &Library::open_at(library)), undoable)
                    }
                    Err(error) => {
                        tgg_app::log(&format!("reading slot states: {error}"));
                        return;
                    }
                };
                if done
                    .send(Done::States {
                        iso,
                        states,
                        undoable,
                    })
                    .is_ok()
                    && let Some(waker) = waker
                {
                    waker();
                }
            })
            .expect("start the slot states thread");
    }

    /// Run `work` on a thread named `name`, and send what it returns back.
    pub fn spawn(&self, name: &str, work: impl FnOnce() -> Done + Send + 'static) {
        if self.inline {
            let _ = self.done.send(work());
            return;
        }
        let (done, waker) = (self.done.clone(), self.waker.clone());
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                if done.send(work()).is_ok()
                    && let Some(waker) = waker
                {
                    waker();
                }
            })
            .expect("start a job's thread");
    }

    /// What wakes the window, for work that reports progress as it goes.
    pub fn waker(&self) -> Option<Waker> {
        self.waker.clone()
    }

    /// The results that have arrived.
    pub fn take(&self) -> Vec<Done> {
        self.results.try_iter().collect()
    }

    /// The next result, waiting up to `timeout` for it; for tests.
    #[cfg(test)]
    pub fn wait(&self, timeout: std::time::Duration) -> Option<Done> {
        self.results.recv_timeout(timeout).ok()
    }
}

/// What each slot of `game` holds.
fn states_of(game: &tgg_app::Game, library: &Library) -> HashMap<MeleeSlot, SlotState> {
    game.slots()
        .into_iter()
        .filter_map(|slot| {
            let bytes = game.read_slot(slot).ok()?;
            Some((slot, SlotState::of(slot, &bytes, library)))
        })
        .collect()
}

/// Once the window runs: keep its waker, and start reading the game.
pub fn setup(app: &mut crate::state::App, waker: Waker) {
    app.jobs.set_waker(waker);
    crate::melee::actions::refresh(app);
    // Outside tests and recordings: offer a report when the last session
    // crashed, and ask once whether a newer version is out (Settings turns
    // that off).
    if app.persist && !app.jobs.inline {
        if tgg_app::log::crashed_before() {
            crate::report::actions::open(app, true);
        }
        if app.settings.check_for_updates.unwrap_or(true) {
            app.jobs.spawn("update check", || {
                Done::Update(tgg_app::update::check(env!("CARGO_PKG_VERSION")))
            });
        }
    }
}

/// Before each frame: hand what's arrived to the actions that own it.
/// True when anything that shows changed.
pub fn poll(app: &mut crate::state::App, _: f64, tx: &mut rune::Tx) -> bool {
    let mut changed = false;
    for refused in std::mem::take(&mut app.stage.rejected) {
        app.tell(refused, true);
        changed = true;
    }
    changed |= crate::editor::external::check(app, tx);
    if let Some(picked) = app.stage.picked.take() {
        crate::editor::actions::select_picked(app, &picked, tx);
        changed = true;
    }
    // An install reports progress by waking the window.
    changed |= app.mods.work.is_some();
    for done in app.jobs.take() {
        changed |= match done {
            Done::States {
                iso,
                states,
                undoable,
            } => crate::melee::actions::states_read(app, &iso, states, undoable),
            Done::Mods(done) => crate::mods::actions::done(app, *done),
            Done::ReportSent(result) => crate::report::actions::sent(app, result),
            Done::Update(newer) => {
                app.update = newer;
                app.update.is_some()
            }
        };
    }
    changed
}
