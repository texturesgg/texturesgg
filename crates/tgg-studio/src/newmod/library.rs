//! What Start a mod asks tgg-mod: the installed tgg-melee's SDK (fetched
//! once when it isn't there), which build tools are installed, starting the
//! mod, and the docs' address. The one place the sheet meets the library.

pub use tgg_mod::{CreatedMod, NewMod as Spec, Progress, Sdk, Tools};

pub use tgg_mod::{docs_url, new_mod, release_sdk, tools};
