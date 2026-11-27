//! Links from textures.gg: `textures-gg://mods/<id>` opens Mods on that
//! mod, `textures-gg://sets/<name>` adds that set and opens it, and
//! `textures-gg://join/<code>` opens Play and joins that room with the
//! active set. A room's invite is `https://textures.gg/join/<code>`
//! ([`invite`]), which chat apps make clickable and the site hands on as
//! `textures-gg://join/<code>`; the app reads that form too, as when it's
//! pasted. A link waits until there's something to open it on: a game
//! chosen (first run picks it), and for a mod or a set, textures.gg's
//! catalog for the tgg-melee in use.
//!
//! macOS hands links to the running app (`on_open_url`). Windows and Linux
//! start the app with the link as an argument; a copy already running is
//! sent it (see [`forward`] and [`listen`]) and the new one quits.

use crate::mods::Registry;
use crate::nav::Place;
use crate::state::App;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;
use tgg_mod::ModId;

/// The scheme the app is registered for.
pub const SCHEME: &str = "textures-gg";

/// The site, whose `/join/<code>` pages open the app on a room.
const SITE: &str = "textures.gg";

/// The invite to the room `code`, to send a friend: a web page, since chat
/// apps don't make links of other schemes. The site opens the app from it.
pub fn invite(code: &str) -> String {
    format!("https://{SITE}/join/{code}")
}

/// What a link asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Mod(ModId),
    Set(String),
    /// Join the room with this code, as direct-connect writes them: letters
    /// in upper case.
    Join(String),
}

/// The link `text` is, if it's one of textures.gg's: the app's own, or an
/// invite to a room on the site. Browsers may add a trailing slash or a
/// query; neither matters.
pub fn parse(text: &str) -> Option<Link> {
    let (scheme, rest) = text.trim().split_once(':')?;
    let rest = rest.strip_prefix("//")?;
    let rest = rest.split(['?', '#']).next()?.trim_end_matches('/');
    if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") {
        return invited(rest);
    }
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let (kind, name) = rest.split_once('/')?;
    match kind.to_ascii_lowercase().as_str() {
        "mods" => name.parse().ok().map(Link::Mod),
        "sets" if is_set_name(name) => Some(Link::Set(name.to_owned())),
        "join" => room_code(name).map(Link::Join),
        _ => None,
    }
}

/// The room a site address (`textures.gg/join/<code>`, after its scheme)
/// invites to. Nothing else on the site opens the app.
fn invited(address: &str) -> Option<Link> {
    let (host, path) = address.split_once('/')?;
    let host = host.to_ascii_lowercase();
    if host != SITE && host.strip_prefix("www.") != Some(SITE) {
        return None;
    }
    let (kind, code) = path.split_once('/')?;
    if !kind.eq_ignore_ascii_case("join") {
        return None;
    }
    room_code(code).map(Link::Join)
}

/// A room's code, as a link spells it: the letters direct-connect makes
/// codes from (no I, L or O, which a keyboard's font confuses), in upper
/// case. Codes are four letters today; a longer one is taken as it comes.
fn room_code(name: &str) -> Option<String> {
    let letters = name.chars().count();
    if !(4..=16).contains(&letters) {
        return None;
    }
    let mut code = String::with_capacity(name.len());
    for letter in name.chars() {
        let letter = letter.to_ascii_uppercase();
        if !letter.is_ascii_uppercase() || matches!(letter, 'I' | 'L' | 'O') {
            return None;
        }
        code.push(letter);
    }
    Some(code)
}

/// A set's name as the registry writes them: lowercase letters, digits and
/// dashes.
fn is_set_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The link among the app's arguments, as Windows and Linux start it.
pub fn in_arguments(arguments: &[String]) -> Option<&str> {
    arguments.iter().map(String::as_str).find(|argument| {
        argument
            .get(..=SCHEME.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(&format!("{SCHEME}:")))
    })
}

/// Open `text`, a link from outside the app: keep it until it can be
/// opened, and open it now if it can.
pub fn open(app: &mut App, text: &str) {
    let Some(link) = parse(text) else {
        app.tell("That link isn't one the app knows.", true);
        return;
    };
    let place = match &link {
        // A room is played from Play, where the sheet shows it.
        Link::Join(_) => Place::Play,
        Link::Mod(_) | Link::Set(_) => Place::Mods,
    };
    app.link = Some(link);
    if app.game.is_some() {
        crate::nav::actions::go(app, place);
    }
    apply(app);
}

/// For `on_open_url` (macOS).
pub fn opened_with(app: &mut App, text: &str, _: &mut rune::Tx) {
    open(app, text);
}

/// Open the link waiting, once it can be. True: what shows changed.
pub fn apply(app: &mut App) -> bool {
    if app.link.is_none() || app.game.is_none() {
        return false;
    }
    // A join starts a room, not an install: it needs tgg-melee, the disc
    // and the sets, not the catalog.
    if matches!(app.link, Some(Link::Join(_))) {
        return join(app);
    }
    if app.nav.place != Place::Mods {
        // First run was showing when it came.
        crate::nav::actions::go(app, Place::Mods);
    }
    match &app.mods.registry {
        Registry::NotAsked | Registry::Asking => return false,
        Registry::Offline | Registry::Failed(_) => {
            app.link = None;
            app.tell("textures.gg couldn't be reached to open the link.", true);
            return true;
        }
        // Without tgg-melee nothing installs yet: Mods says to set it
        // up, and the link opens once it's there.
        Registry::Answered if app.mods.catalog.is_none() || app.mods.port().is_none() => {
            return false;
        }
        Registry::Answered => {}
    }
    let Some(link) = app.link.take() else {
        return false;
    };
    match link {
        // Every join came off above.
        Link::Join(_) => {}
        Link::Mod(id) => {
            if app.mods.newest(&id).is_some() || app.mods.local.get(&id).is_some() {
                crate::mods::actions::show_mod(app, &id);
            } else if app
                .mods
                .catalog
                .as_ref()
                .is_some_and(|catalog| catalog.ids.contains(&id))
            {
                let name = app.mods.name_of(&id);
                app.tell(format!("{name} isn't built for this tgg-melee yet."), true);
            } else {
                app.tell(format!("textures.gg has no mod {id}."), true);
            }
        }
        Link::Set(name) => {
            if app.mods.sets.iter().any(|set| set.name == name) {
                crate::mods::actions::add_set(app, &name);
            } else {
                app.tell(format!("textures.gg has no set {name}."), true);
            }
        }
    }
    true
}

/// Join the room a link names, with the active set, once tgg-melee, the
/// disc and the sets are all there. True: what shows changed.
fn join(app: &mut App) -> bool {
    if app.nav.place != Place::Play {
        crate::nav::actions::go(app, Place::Play);
        return true;
    }
    if app.mods.port().is_none() || !matches!(app.mods.disc, crate::mods::Disc::Found(_)) {
        return false;
    }
    // Without a set read there's nothing to start the game from yet.
    if app.sets.is_vanilla() {
        app.link = None;
        app.tell(
            "Switch from Vanilla to a set with online mods to join a room.",
            true,
        );
        return true;
    }
    if app.sets.active().is_none() {
        return false;
    }
    let Some(Link::Join(code)) = app.link.take() else {
        return false;
    };
    crate::play::actions::join_link(app, &code);
    true
}

/// Where the running app says which port it listens on.
fn port_file() -> Option<PathBuf> {
    Some(tgg_app::log::folder()?.parent()?.join("studio-port"))
}

/// What the running app answers a link with.
const ANSWER: &str = "textures.gg";

/// Send `link` to a copy of the app already running. True: it has it, and
/// this one has nothing to do.
pub fn forward(link: &str) -> bool {
    let Some(port) = port_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| text.trim().parse::<u16>().ok())
    else {
        return false;
    };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(stream, "{}", link.trim()).is_err() {
        return false;
    }
    // Something else may have the port since; only the app answers so.
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == ANSWER
}

/// Take links a later launch sends (see [`forward`]) and open them, with
/// the window brought forward.
pub fn listen(handle: rune::AppHandle<App>) {
    let Some(file) = port_file() else { return };
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else {
        return;
    };
    let Ok(address) = listener.local_addr() else {
        return;
    };
    if std::fs::write(&file, address.port().to_string()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("links".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut reader = BufReader::new(&stream).take(4096);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || parse(&line).is_none() {
                    continue;
                }
                let _ = writeln!(&stream, "{ANSWER}");
                let link = line.trim().to_owned();
                handle.update(move |app, _| open(app, &link));
                handle.open();
            }
        });
    if spawned.is_err() {
        let _ = std::fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_read_as_the_site_writes_them_and_nothing_else_does() {
        let ucf: ModId = "tgg.ucf".parse().unwrap();
        assert_eq!(
            parse("textures-gg://mods/tgg.ucf"),
            Some(Link::Mod(ucf.clone()))
        );
        // As a browser may hand them over.
        assert_eq!(
            parse("textures-gg://mods/tgg.ucf/"),
            Some(Link::Mod(ucf.clone()))
        );
        assert_eq!(
            parse("TEXTURES-GG://mods/tgg.ucf?from=site"),
            Some(Link::Mod(ucf))
        );
        assert_eq!(
            parse("textures-gg://sets/competitive"),
            Some(Link::Set("competitive".into()))
        );
        // A room's code: case doesn't matter, and it comes out as the game
        // writes it.
        assert_eq!(
            parse("textures-gg://join/KFXD"),
            Some(Link::Join("KFXD".into()))
        );
        assert_eq!(
            parse("textures-gg://join/kfxd/"),
            Some(Link::Join("KFXD".into()))
        );
        assert_eq!(
            parse("textures-gg://JOIN/Kfxd?from=site"),
            Some(Link::Join("KFXD".into()))
        );
        // The invite a room's sheet copies, as a chat app or a paste may
        // hand it over.
        for invite in [
            "https://textures.gg/join/KFXD",
            "https://textures.gg/join/kfxd/",
            "http://textures.gg/join/KFXD",
            "https://www.textures.gg/join/KFXD",
            "HTTPS://Textures.GG/Join/kfxd?utm_source=chat#top",
            " https://textures.gg/join/KFXD\n",
        ] {
            assert_eq!(parse(invite), Some(Link::Join("KFXD".into())), "{invite}");
        }
        assert_eq!(parse(&invite("KFXD")), Some(Link::Join("KFXD".into())));
        for not in [
            "https://textures.gg/games/melee/mods/tgg.ucf",
            "https://textures.gg/mods/tgg.ucf",
            "https://textures.gg/sets/competitive",
            "https://textures.gg/join/",
            "https://textures.gg/join/AICO",
            "https://textures.gg/join/KFXD/more",
            "https://textures.gg/x/join/KFXD",
            "https://evil.example/join/KFXD",
            "https://textures.gg.evil.example/join/KFXD",
            "https://evil.textures.gg/join/KFXD",
            "https://textures.gg@evil.example/join/KFXD",
            "https://textures.gg:8443/join/KFXD",
            "ftp://textures.gg/join/KFXD",
            "https:textures.gg/join/KFXD",
            "textures-gg://mods/",
            "textures-gg://mods/../../etc",
            "textures-gg://sets/Competitive Rules",
            "textures-gg://packs/fox",
            "textures-gg:mods/tgg.ucf",
            // Codes leave out the letters a keyboard's font confuses, and
            // are letters only.
            "textures-gg://join/",
            "textures-gg://join/KFX",
            "textures-gg://join/AICO",
            "textures-gg://join/KFXD-7K2Q",
            "textures-gg://join/KFX1",
            "textures-gg://join/KFXDDDDDDDDDDDDDDDDDD",
        ] {
            assert_eq!(parse(not), None, "{not}");
        }
        let arguments = [
            "--iso".to_owned(),
            "a.iso".into(),
            "textures-gg://sets/x".into(),
        ];
        assert_eq!(in_arguments(&arguments), Some("textures-gg://sets/x"));
    }
}
