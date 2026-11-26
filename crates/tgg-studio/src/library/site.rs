//! textures.gg's skins, for Skins' Browse tab: server-side `/api/packs`
//! queries with sort, category/target filters, debounced search, pagination,
//! preview pictures, and downloading a skin into the player's own skins.

use crate::jobs::Done as Job;
use crate::melee::{BrowseSort, Kind, Tab};
use crate::nav::Place;
use crate::state::App;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tgg_app::ids::SkinId;
use tgg_app::library::SkinSource;
use tgg_mod::Net;

/// textures.gg itself, for opening a skin's page or Browse in the browser.
pub const SITE: &str = "https://textures.gg";
/// Where textures.gg keeps pictures.
pub const ASSETS: &str = "https://assets.textures.gg";
/// How many skins each `/api/packs` page asks for.
pub const PAGE_SIZE: usize = 24;
/// Delay after the last search keystroke before querying `/api/packs`.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

/// How the listing answers.
const LIST_LIMIT: u64 = 4 * 1024 * 1024;
const PICTURE_LIMIT: u64 = 8 * 1024 * 1024;
const PACK_LIMIT: u64 = 256 * 1024 * 1024;

/// The site's skins, as Browse shows them.
#[derive(Default)]
pub struct Browse {
    /// The listing; `None` while page 1 is loading or before first load.
    pub items: Option<Result<Vec<Pack>, String>>,
    /// Current loaded page (1-based; 0 before any page loads).
    pub page: u32,
    /// Total matching skins reported by the server.
    pub total: usize,
    /// Total pages reported by the server.
    pub total_pages: u32,
    /// True while a "Load more" request for `page + 1` is in flight.
    pub loading_more: bool,
    /// Error from the last "Load more" request, keeping loaded items on screen.
    pub more_error: Option<String>,
    /// Monotonic query id; responses from older queries are discarded.
    pub generation: u64,
    /// Fighter/stage/UI/audio targets from `GET /api/games/melee/targets`.
    pub targets: Vec<Target>,
    /// Whether targets have already been requested.
    pub targets_requested: bool,
    /// Last trimmed search text sent to `/api/packs`.
    pub last_search: String,
    /// Last trimmed search text observed while polling, for debouncing.
    pub search_observed: String,
    /// When a pending debounced search should fire.
    pub search_due: Option<Instant>,
    /// Token for the active debounce wake timer.
    pub search_timer: u64,
    /// Preview pictures by URL, filled as they arrive.
    pub pictures: HashMap<String, rune::Pixels>,
    /// The skin being downloaded into the player's own, by id.
    pub fetching: Option<String>,
    /// The player's skins each site skin became, by the site's id.
    pub skins: HashMap<String, Vec<SkinId>>,
}

impl Browse {
    /// Whether another page of skins can be loaded from the server.
    pub fn has_more(&self) -> bool {
        self.page > 0 && self.page < self.total_pages
    }

    /// Display name for a target slug, if targets have loaded.
    pub fn target_name(&self, slug: &str) -> Option<&str> {
        self.targets
            .iter()
            .find(|target| target.slug == slug)
            .map(|target| target.name.as_str())
    }
}

/// A fighter, stage, menu, or audio target from `GET /api/games/melee/targets`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub name: String,
    pub slug: String,
    pub category: String,
    #[serde(default)]
    pub pack_count: u32,
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

    /// Its page on textures.gg.
    pub fn path(&self) -> String {
        format!("/games/melee/packs/{}", self.slug)
    }
}

/// One page of the site's listing, deserialized from `/api/packs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedResponse {
    items: Vec<Pack>,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default)]
    total: usize,
    #[serde(default)]
    total_pages: u32,
}

fn default_page() -> u32 {
    1
}

/// Parsed page returned to the UI thread.
pub struct ListedPage {
    pub items: Vec<Pack>,
    pub page: u32,
    pub total: usize,
    pub total_pages: u32,
}

impl From<ListedResponse> for ListedPage {
    fn from(raw: ListedResponse) -> Self {
        let total = if raw.total == 0 && !raw.items.is_empty() {
            raw.items.len()
        } else {
            raw.total
        };
        let total_pages = if raw.total_pages == 0 && !raw.items.is_empty() {
            1
        } else {
            raw.total_pages
        };
        Self {
            items: raw.items,
            page: raw.page.max(1),
            total,
            total_pages,
        }
    }
}

/// What a Browse job found.
pub enum Done {
    /// One page of skins from `/api/packs`.
    Listed {
        generation: u64,
        append: bool,
        result: Result<ListedPage, String>,
    },
    /// Targets from `/api/games/melee/targets`.
    Targets(Result<Vec<Target>, String>),
    /// Debounce timer expired; check if the search query should fire.
    SearchDue(u64),
    /// A skin's files, downloaded into this folder, or why not.
    Fetched {
        pack: String,
        title: String,
        files: Result<Vec<PathBuf>, String>,
    },
}

/// Build the `/api/packs` request URL for the active Browse query.
pub fn packs_url(
    api: &str,
    sort: BrowseSort,
    kind: Option<Kind>,
    target: Option<&str>,
    search: &str,
    page: u32,
) -> String {
    let mut url = format!(
        "{api}/api/packs?game=melee&sortBy={}&page={page}&pageSize={PAGE_SIZE}",
        sort.api_value()
    );
    if let Some(category) = kind.and_then(Kind::api_category) {
        url.push_str("&category=");
        url.push_str(category);
    }
    if let Some(slug) = target.filter(|slug| !slug.is_empty()) {
        url.push_str("&target=");
        url.push_str(&encode_query_value(slug));
    }
    let trimmed = search.trim();
    if !trimmed.is_empty() {
        url.push_str("&search=");
        url.push_str(&encode_query_value(trimmed));
    }
    url
}

/// Build the matching textures.gg web Browse URL for "Open on textures.gg".
pub fn browse_web_url(
    sort: BrowseSort,
    kind: Option<Kind>,
    target: Option<&str>,
    search: &str,
) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(value) = sort.web_value() {
        params.push(format!("sort={value}"));
    }
    if let Some(value) = kind.and_then(Kind::web_type) {
        params.push(format!("type={value}"));
    }
    if let Some(slug) = target.filter(|slug| !slug.is_empty()) {
        params.push(format!("target={}", encode_query_value(slug)));
    }
    let trimmed = search.trim();
    if !trimmed.is_empty() {
        params.push(format!("q={}", encode_query_value(trimmed)));
    }
    if params.is_empty() {
        format!("{SITE}/games/melee")
    } else {
        format!("{SITE}/games/melee?{}", params.join("&"))
    }
}

/// Percent-encode a single query parameter value (RFC 3986 unreserved chars).
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

/// Load Browse's first page (and targets) unless already loaded for the
/// current search text.
pub fn load(app: &mut App) {
    load_targets(app);
    let search = app.melee.search.text.trim();
    let browse = &app.library_page.browse;
    if browse.items.is_some() && browse.last_search == search {
        return;
    }
    query(app);
}

/// Run a fresh page-1 `/api/packs` query for the current sort, kind, target,
/// and search text.
pub fn query(app: &mut App) {
    load_targets(app);
    let sort = app.melee.sort;
    let kind = app.melee.kind;
    let target = app.melee.target.clone();
    let search = app.melee.search.text.trim().to_owned();

    let browse = &mut app.library_page.browse;
    browse.generation += 1;
    let generation = browse.generation;
    browse.items = None;
    browse.page = 0;
    browse.total = 0;
    browse.total_pages = 0;
    browse.loading_more = false;
    browse.more_error = None;
    browse.last_search = search.clone();
    browse.search_observed = search.clone();
    browse.search_due = None;

    spawn_page(app, generation, false, sort, kind, target, search, 1);
}

/// Fetch the next page of the active Browse query and append its items.
pub fn load_more(app: &mut App) {
    let browse = &mut app.library_page.browse;
    if !browse.has_more() || browse.loading_more {
        return;
    }
    browse.loading_more = true;
    browse.more_error = None;
    let generation = browse.generation;
    let next_page = browse.page + 1;
    let sort = app.melee.sort;
    let kind = app.melee.kind;
    let target = app.melee.target.clone();
    let search = browse.last_search.clone();

    spawn_page(app, generation, true, sort, kind, target, search, next_page);
}

/// Debounce search typing on Skins Browse: wait 300 ms after the last edit
/// before sending a fresh `/api/packs` request.
pub fn tick(app: &mut App) {
    if app.nav.place != Place::Skins || app.melee.tab != Tab::Browse {
        return;
    }
    if app.library_page.browse.generation == 0 {
        return;
    }
    let current = app.melee.search.text.trim().to_owned();
    let browse = &mut app.library_page.browse;
    if current != browse.search_observed {
        browse.search_observed = current;
        browse.search_due = Some(Instant::now() + SEARCH_DEBOUNCE);
        browse.search_timer += 1;
        let timer = browse.search_timer;
        if !app.jobs.inline && app.persist {
            app.jobs.spawn("skins search debounce", move || {
                std::thread::sleep(SEARCH_DEBOUNCE);
                Job::Skins(Box::new(Done::SearchDue(timer)))
            });
        }
    }
    if app
        .library_page
        .browse
        .search_due
        .is_some_and(|due| Instant::now() >= due)
    {
        app.library_page.browse.search_due = None;
        query(app);
    }
}

fn load_targets(app: &mut App) {
    let browse = &mut app.library_page.browse;
    if browse.targets_requested {
        return;
    }
    browse.targets_requested = true;
    let net = app.mods.net.clone();
    app.jobs.spawn("skin targets", move || {
        let url = format!("{}/api/games/melee/targets", net.api_url());
        let targets = get_json::<Vec<Target>>(&*net, &url);
        Job::Skins(Box::new(Done::Targets(targets)))
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_page(
    app: &mut App,
    generation: u64,
    append: bool,
    sort: BrowseSort,
    kind: Option<Kind>,
    target: Option<String>,
    search: String,
    page: u32,
) {
    let net = app.mods.net.clone();
    app.jobs.spawn("skins", move || {
        let url = packs_url(net.api_url(), sort, kind, target.as_deref(), &search, page);
        let result = get_json::<ListedResponse>(&*net, &url).map(ListedPage::from);
        Job::Skins(Box::new(Done::Listed {
            generation,
            append,
            result,
        }))
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
        Done::Listed {
            generation,
            append,
            result,
        } => {
            if generation != app.library_page.browse.generation {
                return false;
            }
            if append {
                app.library_page.browse.loading_more = false;
                match result {
                    Ok(page) => {
                        let pictures: Vec<String> =
                            page.items.iter().filter_map(Pack::preview).collect();
                        let browse = &mut app.library_page.browse;
                        browse.page = page.page;
                        browse.total = page.total;
                        browse.total_pages = page.total_pages;
                        browse.more_error = None;
                        if let Some(Ok(existing)) = &mut browse.items {
                            let seen: HashSet<String> =
                                existing.iter().map(|pack| pack.id.clone()).collect();
                            for pack in page.items {
                                if !seen.contains(&pack.id) {
                                    existing.push(pack);
                                }
                            }
                        } else {
                            browse.items = Some(Ok(page.items));
                        }
                        for url in pictures {
                            request_picture(app, &url);
                        }
                    }
                    Err(error) => {
                        app.library_page.browse.more_error = Some(error);
                    }
                }
            } else {
                match result {
                    Ok(page) => {
                        let pictures: Vec<String> =
                            page.items.iter().filter_map(Pack::preview).collect();
                        let browse = &mut app.library_page.browse;
                        browse.page = page.page;
                        browse.total = page.total;
                        browse.total_pages = page.total_pages;
                        browse.more_error = None;
                        browse.items = Some(Ok(page.items));
                        for url in pictures {
                            request_picture(app, &url);
                        }
                    }
                    Err(error) => {
                        let browse = &mut app.library_page.browse;
                        browse.page = 0;
                        browse.total = 0;
                        browse.total_pages = 0;
                        browse.items = Some(Err(error));
                    }
                }
            }
        }
        Done::Targets(Ok(targets)) => {
            app.library_page.browse.targets = targets;
        }
        Done::Targets(Err(_)) => {}
        Done::SearchDue(timer) => {
            let fire = app.nav.place == Place::Skins
                && app.melee.tab == Tab::Browse
                && app.library_page.browse.search_timer == timer
                && app.library_page.browse.search_due.is_some();
            if fire {
                app.library_page.browse.search_due = None;
                query(app);
            } else {
                return false;
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
                Err(error) => app.tell(format!("Couldn't download {title}: {error}."), true),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_url_encodes_sort_category_target_search_and_page() {
        assert_eq!(
            packs_url(
                "https://api.textures.gg",
                BrowseSort::Hot,
                None,
                None,
                "",
                1
            ),
            "https://api.textures.gg/api/packs?game=melee&sortBy=hot&page=1&pageSize=24"
        );
        assert_eq!(
            packs_url(
                "https://api.textures.gg",
                BrowseSort::Newest,
                Some(Kind::Fighters),
                Some("fox"),
                "  anime & blue  ",
                3
            ),
            "https://api.textures.gg/api/packs?game=melee&sortBy=newest&page=3&pageSize=24&category=character&target=fox&search=anime%20%26%20blue"
        );
    }

    #[test]
    fn browse_web_url_matches_site_query_params() {
        assert_eq!(
            browse_web_url(BrowseSort::Hot, None, None, ""),
            "https://textures.gg/games/melee"
        );
        assert_eq!(
            browse_web_url(
                BrowseSort::Newest,
                Some(Kind::Stages),
                Some("battlefield"),
                "night sky"
            ),
            "https://textures.gg/games/melee?sort=new&type=stage&target=battlefield&q=night%20sky"
        );
    }
}
