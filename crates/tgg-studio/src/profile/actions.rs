//! Opening You: reading the profile and its lists from textures.gg, their
//! pictures, adding a pack to the library, and going from it to what was
//! made.

use super::{ASSETS, CodeMod, Done, MELEE, Pack, PackPage, Page, Profile, SHOWN, SITE, Tab, User};
use crate::jobs::Done as Job;
use crate::nav::{Place, actions as nav};
use crate::state::App;
use rune::Tx;
use serde::de::DeserializeOwned;
use tgg_mod::{ModId, Net, NetError};

/// The most a list or a profile answers with.
const LIST_LIMIT: u64 = 1024 * 1024;
/// The most a picture or a pack downloads.
const PICTURE_LIMIT: u64 = 8 * 1024 * 1024;
const PACK_LIMIT: u64 = 256 * 1024 * 1024;
/// The most pictures You fetches in a session.
const MOST_PICTURES: usize = 200;

/// You shows: read the mods folder afresh, ask for the skins' pictures, and
/// read the profile from textures.gg when it's someone else's or not read.
pub fn opened(app: &mut App) {
    crate::mods::actions::read_disk(app);
    crate::library::actions::request_renders(app);
    let Some(handle) = app
        .signin
        .who
        .as_ref()
        .map(|who| who.account.handle.clone())
    else {
        return;
    };
    if app.profile.of.as_deref() == Some(handle.as_str()) && app.profile.user.is_some() {
        return;
    }
    read(app, handle);
}

/// Read `handle`'s profile and lists again.
pub fn refresh(app: &mut App) {
    if let Some(handle) = app.profile.of.clone() {
        read(app, handle);
    }
}

fn read(app: &mut App, handle: String) {
    let tab = app.profile.tab;
    app.profile = Profile {
        of: Some(handle.clone()),
        tab,
        ..Profile::default()
    };
    let net = app.mods.net.clone();
    app.jobs.spawn("profile", move || {
        let api = format!("{}/api", net.api_url());
        let user = format!("{api}/users/{handle}");
        Job::Profile(Box::new(Done::Read {
            user: Box::new(fetch::<User>(&*net, &user)),
            packs: fetch::<Page<Pack>>(&*net, &format!("{user}/uploads?pageSize={SHOWN}"))
                .map(|page| page.items),
            likes: fetch::<Page<Pack>>(&*net, &format!("{user}/likes?pageSize={SHOWN}"))
                .map(|page| page.items),
            mods: code_mods(&*net, &api, &handle),
            handle,
        }))
    });
}

/// Their code mods. Not on the site yet (it answers 404): none.
fn code_mods(net: &dyn Net, api: &str, handle: &str) -> Result<Vec<CodeMod>, String> {
    let url = format!("{api}/code-mods?owner={handle}&pageSize={SHOWN}");
    match net.get(&url, LIST_LIMIT) {
        Err(NetError::NotFound { .. }) => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
        Ok(body) => serde_json::from_slice::<Page<CodeMod>>(&body)
            .map(|page| page.items)
            .map_err(|error| error.to_string()),
    }
}

fn fetch<T: DeserializeOwned>(net: &dyn Net, url: &str) -> Result<T, String> {
    let body = net
        .get(url, LIST_LIMIT)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Read {
            handle,
            user,
            packs,
            likes,
            mods,
        } => {
            if app.profile.of.as_deref() != Some(handle.as_str()) {
                return false;
            }
            let mut pictures: Vec<String> = Vec::new();
            if let Ok(user) = &*user {
                pictures.extend(user.image.clone());
            }
            for pack in packs.iter().chain(likes.iter()).flatten() {
                pictures.extend(thumbnail(pack));
            }
            app.profile.user = Some(*user);
            app.profile.packs = Some(packs);
            app.profile.likes = Some(likes);
            app.profile.mods = Some(mods);
            for url in pictures {
                request_picture(app, &url);
            }
            true
        }
        Done::Fetched { title, file } => {
            app.profile.fetching = None;
            match file {
                Ok(path) => crate::library::actions::add(app, &[path]),
                Err(error) => app.tell(format!("Couldn't download {title}: {error}"), true),
            }
            true
        }
    }
}

/// Where a pack's picture is.
pub fn thumbnail(pack: &Pack) -> Option<String> {
    pack.thumbnail_key
        .as_ref()
        .map(|key| format!("{ASSETS}/{key}_sm.webp"))
}

/// Fetch the picture at `url` once, on a thread of its own; it shows as
/// soon as it's decoded. Up to `MOST_PICTURES`; in recordings, before the
/// next frame.
fn request_picture(app: &mut App, url: &str) {
    if app.profile.pictures.contains_key(url) || app.profile.pictures.len() >= MOST_PICTURES {
        return;
    }
    let pixels = rune::Pixels::new();
    app.profile.pictures.insert(url.to_owned(), pixels.clone());
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
        .name("profile picture".into())
        .spawn(fetch)
        .expect("start a picture's thread");
}

/// Show one of the published lists.
pub fn show_tab(app: &mut App, tab: Tab) {
    app.profile.tab = tab;
}

/// Download `pack` and look at it as the player's own files would be: Review
/// says where each skin goes before any joins the library.
pub fn add_pack(app: &mut App, pack: &Pack) {
    if app.profile.fetching.is_some() || !pack.for_melee() {
        return;
    }
    app.profile.fetching = Some(pack.id.clone());
    let net = app.mods.net.clone();
    let folder = app.data.library.with_file_name("downloads");
    let (id, slug, title) = (pack.id.clone(), pack.slug.clone(), pack.title.clone());
    app.jobs.spawn("pack download", move || {
        let file = download(&*net, &folder, &id, &slug);
        Job::Profile(Box::new(Done::Fetched { title, file }))
    });
}

/// The pack `id` saved in `folder` under the name it downloads as: a lone
/// file is named for its slot ("PlZdGr.dat"), several come zipped.
fn download(
    net: &dyn Net,
    folder: &std::path::Path,
    id: &str,
    slug: &str,
) -> Result<std::path::PathBuf, String> {
    let api = format!("{}/api", net.api_url());
    let bytes = net
        .get(&format!("{api}/packs/by-id/{id}/download"), PACK_LIMIT)
        .map_err(|error| error.to_string())?;
    let name = if bytes.starts_with(b"PK\x03\x04") {
        format!("{slug}.zip")
    } else {
        let page: PackPage = fetch(net, &format!("{api}/games/{MELEE}/packs/{slug}"))?;
        let file = page
            .mods
            .into_iter()
            .next()
            .ok_or("the pack has no files")?;
        // Only the name: never a path the site could steer elsewhere.
        std::path::Path::new(&file.file_name)
            .file_name()
            .ok_or("its file has no name")?
            .to_string_lossy()
            .into_owned()
    };
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let path = folder.join(name);
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Open a page of textures.gg in the browser.
pub fn open_site(app: &mut App, path: &str, tx: &mut Tx) {
    if let Err(error) = tx.open_url(&format!("{SITE}{path}")) {
        app.tell(format!("Couldn't open textures.gg: {error}"), true);
    }
}

/// The site's address for `pack`.
pub fn pack_path(pack: &Pack) -> String {
    format!(
        "/games/{}/packs/{}",
        pack.game().unwrap_or(MELEE),
        pack.slug
    )
}

/// Show the mod `id`, one the player is making, among them on Mods.
pub fn open_mod(app: &mut App, id: &ModId) {
    nav::go(app, Place::Mods);
    crate::mods::actions::show_tab(app, crate::mods::Tab::Installed);
    crate::mods::actions::show_made(app, true);
    if app.mods.selected.as_ref() != Some(id) {
        crate::mods::actions::select(app, id);
    }
}

/// Show the skins the player made, among their skins on Skins.
pub fn open_skins(app: &mut App) {
    crate::melee::actions::show_mine(app);
    crate::library::actions::show_filter(app, crate::library::Filter::Made);
}
