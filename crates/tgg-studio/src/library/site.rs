//! textures.gg's skins, for Skins' Browse tab: the site's listing of Melee
//! skins with a preview each, and taking one into the player's own skins
//! when they turn it on. A skin downloaded from the site goes through the
//! library like any other file, so Using and its picture work the same.

use crate::jobs::Done as Job;
use crate::state::App;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tgg_app::ids::SkinId;
use tgg_app::library::SkinSource;
use tgg_mod::Net;

/// textures.gg itself, for opening a skin's page in the browser.
pub const SITE: &str = "https://textures.gg";
/// Where textures.gg keeps pictures.
pub const ASSETS: &str = "https://assets.textures.gg";
/// How many skins Browse lists: the site has the rest.
pub const LISTED: usize = 30;

/// How the listing answers.
const LIST_LIMIT: u64 = 4 * 1024 * 1024;
const PICTURE_LIMIT: u64 = 8 * 1024 * 1024;
const PACK_LIMIT: u64 = 256 * 1024 * 1024;

/// The site's skins, as Browse shows them.
#[derive(Default)]
pub struct Browse {
    /// The listing; `None` until it's asked for.
    pub items: Option<Result<Vec<Pack>, String>>,
    /// Preview pictures by URL, filled as they arrive.
    pub pictures: HashMap<String, rune::Pixels>,
    /// The skin being downloaded into the player's own, by id.
    pub fetching: Option<String>,
    /// The player's skins each site skin became, by the site's id.
    pub skins: HashMap<String, Vec<SkinId>>,
}

/// A skin on textures.gg, as `GET /api/packs?game=melee` gives it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub id: String,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub thumbnail_key: Option<String>,
    #[serde(default)]
    pub creator_name: Option<String>,
    #[serde(default)]
    pub target_name: Option<String>,
    #[serde(default)]
    pub target_slug: Option<String>,
    #[serde(default)]
    pub target_category: Option<String>,
}

impl Pack {
    /// Where its preview is.
    pub fn preview(&self) -> Option<String> {
        self.thumbnail_key
            .as_ref()
            .map(|key| format!("{ASSETS}/{key}_sm.webp"))
    }

    /// What it changes: a fighter, a stage, a menu.
    pub fn kind(&self) -> crate::melee::Kind {
        match self.target_category.as_deref() {
            Some("character") => crate::melee::Kind::Fighters,
            Some("stage") => crate::melee::Kind::Stages,
            Some("audio") => crate::melee::Kind::Effects,
            _ => crate::melee::Kind::Menus,
        }
    }

    /// Its page on textures.gg.
    pub fn path(&self) -> String {
        format!("/games/melee/packs/{}", self.slug)
    }
}

/// One page of the site's listing.
#[derive(Debug, Deserialize)]
struct Listed {
    items: Vec<Pack>,
}

/// What a Browse job found.
pub enum Done {
    /// The site's skins, read.
    Listed(Result<Vec<Pack>, String>),
    /// A skin's files, downloaded into this folder, or why not.
    Fetched {
        pack: String,
        title: String,
        files: Result<Vec<PathBuf>, String>,
    },
}

/// Read the site's Melee skins, newest first, unless they've been read.
pub fn load(app: &mut App) {
    load_again(app, false);
}

/// Read the site's Melee skins again.
pub fn load_again(app: &mut App, again: bool) {
    let browse = &mut app.library_page.browse;
    if browse.items.is_some() && !again {
        return;
    }
    browse.items = None;
    let net = app.mods.net.clone();
    app.jobs.spawn("skins", move || {
        let url = format!(
            "{}/api/packs?game=melee&sortBy=hot&pageSize={LISTED}",
            net.api_url()
        );
        let listed = net
            .get(&url, LIST_LIMIT)
            .map_err(|error| error.to_string())
            .and_then(|body| {
                serde_json::from_slice::<Listed>(&body)
                    .map(|page| page.items)
                    .map_err(|error| error.to_string())
            });
        Job::Skins(Box::new(Done::Listed(listed)))
    });
}

/// Download the skin `id` and keep its files among the player's skins; then
/// `then` does what the player asked with them.
pub fn fetch(app: &mut App, pack: &Pack, then: Then) {
    if app.library_page.browse.fetching.is_some() {
        return;
    }
    app.library_page.browse.fetching = Some(pack.id.clone());
    app.library_page.pending = Some((pack.clone(), then));
    let net = app.mods.net.clone();
    let folder = app.data.library.with_file_name("downloads");
    let (id, slug, title) = (pack.id.clone(), pack.slug.clone(), pack.title.clone());
    app.jobs.spawn("skin download", move || {
        let files = download(&*net, &folder, &id, &slug);
        Job::Skins(Box::new(Done::Fetched {
            pack: id,
            title,
            files,
        }))
    });
}

/// What the player asked for, once a skin from the site is in their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// Leave it in their skins, not on in a game.
    Keep,
    /// Turn it on in tgg-melee.
    Use,
    /// Write it into their Slippi disc.
    Slippi,
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Listed(items) => {
            let pictures: Vec<String> = match &items {
                Ok(packs) => packs.iter().filter_map(Pack::preview).collect(),
                Err(_) => Vec::new(),
            };
            app.library_page.browse.items = Some(items);
            for url in pictures {
                request_picture(app, &url);
            }
        }
        Done::Fetched { pack, title, files } => {
            app.library_page.browse.fetching = None;
            let pending = app
                .library_page
                .pending
                .take()
                .filter(|(waiting, _)| waiting.id == pack);
            let then = pending.map_or(Then::Keep, |(_, then)| then);
            match files {
                Ok(paths) => keep(app, &pack, &title, &paths, then),
                Err(error) => app.tell(format!("Couldn't get {title}: {error}."), true),
            }
        }
    }
    true
}

/// Keep the downloaded `paths` as the player's skins, then do `then` with
/// the ones that went into a slot.
fn keep(app: &mut App, pack: &str, title: &str, paths: &[PathBuf], then: Then) {
    let (items, rejected): (Vec<_>, Vec<_>) = app
        .library
        .inspect_files(paths, melee_dat::MeleeReferenceCatalog::checked_in())
        .into_iter()
        .partition(Result::is_ok);
    let mut stored: Vec<SkinId> = Vec::new();
    for candidate in items.into_iter().flatten() {
        let name = candidate.name.clone();
        let slot = candidate.slot;
        match app.library.store(&candidate, slot, SkinSource::Imported) {
            Ok(skin) => stored.push(skin.id),
            Err(error) => app.tell(format!("Couldn't add {name}: {error}."), true),
        }
    }
    if stored.is_empty() {
        let why = rejected
            .into_iter()
            .filter_map(Result::err)
            .next()
            .map_or_else(String::new, |rejected| format!(" {}", rejected.reason));
        app.tell(format!("{title} has no skins this app can use.{why}"), true);
        return;
    }
    app.library_page
        .browse
        .skins
        .insert(pack.to_owned(), stored.clone());
    match then {
        Then::Keep => app.tell(format!("{title} is in your skins."), false),
        Then::Use => crate::library::actions::turn_on(app, &stored, true),
        Then::Slippi => crate::library::actions::write_into_slippi(app, &stored, true),
    }
    crate::library::actions::request_renders(app);
}

/// The skins Browse fetched for `pack` this session.
pub fn kept<'a>(app: &'a App, pack: &str) -> &'a [SkinId] {
    app.library_page
        .browse
        .skins
        .get(pack)
        .map_or(&[], Vec::as_slice)
}

/// Ask for the picture at `url` once; it shows as soon as it's decoded.
fn request_picture(app: &mut App, url: &str) {
    let browse = &mut app.library_page.browse;
    if browse.pictures.contains_key(url) {
        return;
    }
    let pixels = rune::Pixels::new();
    browse.pictures.insert(url.to_owned(), pixels.clone());
    let (net, url) = (app.mods.net.clone(), url.to_owned());
    let fetch = move || {
        let decoded = net
            .get(&url, PICTURE_LIMIT)
            .map_err(|error| error.to_string())
            .and_then(|bytes| image::load_from_memory(&bytes).map_err(|error| error.to_string()));
        match decoded {
            Ok(image) => {
                let rgba = image.to_rgba8();
                pixels.set(rgba.width(), rgba.height(), rgba.into_raw());
            }
            Err(error) => tgg_app::log(&format!("the picture {url}: {error}")),
        }
    };
    if app.jobs.inline {
        return fetch();
    }
    std::thread::Builder::new()
        .name("skin picture".into())
        .spawn(fetch)
        .expect("start a picture's thread");
}

/// The pack `id` saved in `folder`: a lone file keeps its slot's name, and
/// several come zipped.
fn download(net: &dyn Net, folder: &Path, id: &str, slug: &str) -> Result<Vec<PathBuf>, String> {
    let api = format!("{}/api", net.api_url());
    let bytes = net
        .get(&format!("{api}/packs/by-id/{id}/download"), PACK_LIMIT)
        .map_err(|error| error.to_string())?;
    let name = if bytes.starts_with(b"PK\x03\x04") {
        format!("{slug}.zip")
    } else {
        let page: PackPage = get_json(net, &format!("{api}/games/melee/packs/{slug}"))?;
        let file = page.mods.into_iter().next().ok_or("it has no files")?;
        // Only the name: never a path the site could steer elsewhere.
        Path::new(&file.file_name)
            .file_name()
            .ok_or("its file has no name")?
            .to_string_lossy()
            .into_owned()
    };
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let path = folder.join(name);
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(vec![path])
}

/// A skin's page: its files, which say what a lone one is called.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackPage {
    mods: Vec<PackFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackFile {
    file_name: String,
}

fn get_json<T: serde::de::DeserializeOwned>(net: &dyn Net, url: &str) -> Result<T, String> {
    let body = net
        .get(url, LIST_LIMIT)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}
