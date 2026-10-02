//! A DAT open in the app: the one description of it the game page's preview,
//! the launch, and the editor all pass around.

use crate::viewport::Viewport;
use crate::{Costume, Error, References, load_model};
use gpui::{AppContext, Context, Entity};
use melee_dat::{MeleeReferenceStore, MeleeSlot};
use std::path::PathBuf;
use std::rc::Rc;

/// A DAT open in the app: its bytes, what to call it, where it lives, and
/// the viewport showing it.
#[derive(Clone)]
pub(crate) struct OpenFile {
    /// Its file name (`PlFcRe.dat`), suggested by save and export dialogs.
    pub name: String,
    pub title: String,
    pub bytes: Vec<u8>,
    /// Where it was read from on disk, and where Save writes; `None` for a
    /// file in the game.
    pub path: Option<PathBuf>,
    /// The game slot it fills; saving installs into it.
    pub slot: Option<MeleeSlot>,
    /// Its reference files, when it is a stock costume.
    pub store: Option<Rc<MeleeReferenceStore>>,
    pub viewport: Entity<Viewport>,
}

impl OpenFile {
    /// Parse `bytes` and show the model in a viewport of its own.
    pub fn show<T: 'static>(
        name: &str,
        bytes: Vec<u8>,
        references: Option<&References>,
        cx: &mut Context<T>,
    ) -> Result<Self, Error> {
        let loaded = load_model(name, &bytes, references)?;
        Ok(Self::of(
            Costume {
                loaded,
                name: name.to_owned(),
                dat: bytes,
                path: None,
            },
            cx,
        ))
    }

    /// A costume read before the window opened, in a viewport of its own.
    pub fn of<T: 'static>(costume: Costume, cx: &mut Context<T>) -> Self {
        let Costume {
            loaded,
            name,
            dat,
            path,
        } = costume;
        Self {
            name,
            title: loaded.title,
            bytes: dat,
            path,
            slot: None,
            store: loaded.store,
            viewport: cx.new(|cx| Viewport::new(loaded.model, cx.focus_handle())),
        }
    }
}
