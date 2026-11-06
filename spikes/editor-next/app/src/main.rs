//! tgg-next: the textures.gg editor's Catalog design on Rune, as a proof of
//! concept. One fighter from the player's disc, drawn by hsd-render on Rune's
//! own device inside Rune's layout, with the shell around it.
//!
//! `cargo run --release -- --iso path/to/melee.iso`, or set `TGG_MELEE_ISO`.

mod fighter;
mod viewport;

use hsd_render::{CameraView, Orbit};
use rune::tok::{Family, TextStyle, motion};
use rune::{AppSpec, Color, Cx, El, Phase, col, effect, el, row, text};
use std::path::PathBuf;
use viewport::{Stats, VIEWPORT, Viewport};

static SANS: &[&[u8]] = &[
    include_bytes!("../../mocks/fonts/SchibstedGrotesk-400.ttf"),
    include_bytes!("../../mocks/fonts/SchibstedGrotesk-500.ttf"),
    include_bytes!("../../mocks/fonts/SchibstedGrotesk-600.ttf"),
    include_bytes!("../../mocks/fonts/SchibstedGrotesk-700.ttf"),
    include_bytes!("../../mocks/fonts/SchibstedGrotesk-800.ttf"),
    include_bytes!("../../mocks/fonts/DMMono-Regular.ttf"),
    include_bytes!("../../mocks/fonts/DMMono-Medium.ttf"),
];

/// Fox's costumes, in the game's order.
const COSTUMES: [(&str, &str); 4] = [("Default", "PlFxNr.dat"), ("Orange", "PlFxOr.dat"), ("Lavender", "PlFxLa.dat"), ("Green", "PlFxGr.dat")];

pub struct State {
    pub orbit: Orbit,
    pub playing: bool,
    pub costume: usize,
    pub stats: Stats,
    /// What the view shows, refreshed twice a second.
    pub shown: Stats,
    shown_at: f64,
    pub error: Option<String>,
}

impl State {
    pub fn costume_file(&self) -> String {
        COSTUMES[self.costume].1.to_owned()
    }
}

fn init() -> State {
    State { orbit: CameraView::Front.orbit(), playing: true, costume: 0, stats: Stats::default(), shown: Stats::default(), shown_at: 0.0, error: None }
}

// ---------------------------------------------------------------- design tokens (Gallery)

/// An sRGB hex colour, as the design tokens write it.
fn c(rgb: u32) -> Color {
    let ch = |shift: u32| rune::color::srgb_decode(((rgb >> shift) & 0xff) as f32 / 255.0);
    Color { r: ch(16), g: ch(8), b: ch(0), a: 1.0 }
}
struct T;
impl T {
    fn sunk() -> Color { c(0x0c0c0b) }
    fn bg() -> Color { c(0x111110) }
    fn surface() -> Color { c(0x1b1b19) }
    fn ink() -> Color { c(0xf2efe8) }
    fn muted() -> Color { c(0xa8a39a) }
    fn faint() -> Color { c(0x77726a) }
    fn line() -> Color { c(0x2f2e2a) }
    fn line_strong() -> Color { c(0x3a3934) }
    fn accent() -> Color { c(0xf5c84b) }
    fn plate() -> Color { c(0x050509) }
}
const SANS_FAMILY: Family = Family::Named("Schibsted Grotesk");
const BODY: TextStyle = TextStyle::new(13.5, 400.0).family(SANS_FAMILY);
const STRONG: TextStyle = TextStyle::new(13.5, 600.0).family(SANS_FAMILY);
const PANE_TITLE: TextStyle = TextStyle::new(13.5, 700.0).family(SANS_FAMILY);
const WORDMARK: TextStyle = TextStyle::new(15.0, 800.0).family(SANS_FAMILY).tracking(-0.45);
const MONO: TextStyle = TextStyle::new(11.5, 400.0).family(Family::Named("DM Mono"));

// ---------------------------------------------------------------- view

fn view(s: &State, _: &mut Cx<State>) -> El {
    row().w_full().h_full().fill(T::bg()).child(sidebar()).child(place(s))
}

fn sidebar() -> El {
    let place = |label: &str, on: bool| {
        row().h(32.0).px(10.0).radius(4.0).items_center()
            .fill(if on { Color::WHITE.alpha(0.06) } else { Color::TRANSPARENT })
            .child(text(label).font(STRONG).color(if on { T::ink() } else { T::muted() }))
    };
    let sidebar = col().w(220.0).h_full().fill(T::sunk())
        .child(row().h(44.0).pl(18.0).items_center().window_drag().child(text("textures.gg").font(WORDMARK).color(T::ink())))
        .child(col().px(8.0).py(6.0).gap(2.0).child(place("Melee", true)).child(place("Library", false)).child(place("Mods", false)));
    row().h_full().child(sidebar).child(el().w(1.0).h_full().fill(T::line()))
}

fn place(s: &State) -> El {
    let crumbs = row().gap(8.0).items_center()
        .child(text("Melee").font(STRONG).color(T::muted()))
        .child(text("/").font(BODY).color(T::faint()))
        .child(text("Fox").font(STRONG).color(T::ink()));
    let title = row().h(44.0).px(20.0).items_center().window_drag().child(crumbs);
    col().grow(1.0).h_full().min_w(0.0).child(title).child(subject(s))
}

/// The fighter on the stage, the timeline under it, the pane column beside.
fn subject(s: &State) -> El {
    let stats = s.shown;
    let corner = format!("{} · {}×{} px · {:.2} ms", COSTUMES[s.costume].1, stats.size.0, stats.size.1, stats.cpu_ms);
    // The slot itself: Rune lays it out; the compositor draws the fighter in it.
    let stage = effect(VIEWPORT).key("stage").grow(1.0).h_full().min_w(0.0)
        .cursor(rune::Cursor::Grab)
        .on_drag(|s: &mut State, d: &mut rune::Drag, _| {
            if d.phase == Phase::Move {
                s.orbit.yaw -= f64::from(d.dx) * 0.01;
                s.orbit.pitch = (s.orbit.pitch + f64::from(d.dy) * 0.01).clamp(-1.4, 1.4);
            }
        });
    let mut plate = el().grow(1.0).h_full().min_w(0.0).fill(T::plate()).border(1.0, T::line())
        .child(stage)
        .child(text(corner).font(MONO).color(T::faint()).absolute(14.0, 12.0));
    if let Some(error) = &s.error {
        plate = plate.child(text(error.clone()).font(BODY).color(c(0xf07167)).absolute(14.0, 36.0));
    }
    let play = el().key("play").size(26.0, 26.0).radius(4.0).fill(T::ink()).center()
        .child(text(if s.playing { "II" } else { "▶" }).font(STRONG).color(T::bg()))
        .on_press(|s: &mut State| s.playing = !s.playing);
    let timeline = row().h(44.0).px(12.0).gap(12.0).items_center().fill(T::surface()).border(1.0, T::line_strong()).radius(4.0)
        .child(play).child(text("Wait").font(STRONG).color(T::ink()))
        .child(el().grow(1.0).h(2.0).fill(T::line_strong()));
    let stage_col = col().grow(1.0).h_full().min_w(0.0).gap(12.0).child(plate).child(timeline);
    row().grow(1.0).w_full().min_h(0.0).px(16.0).pb(16.0).gap(16.0).child(stage_col).child(panes(s))
}

fn panes(s: &State) -> El {
    let rows = COSTUMES.iter().enumerate().map(|(i, (name, file))| {
        let on = i == s.costume;
        row().key(format!("costume-{i}")).h(34.0).px(12.0).gap(10.0).items_center()
            .fill(if on { T::accent().alpha(0.12) } else { Color::TRANSPARENT })
            .motion(motion::SNAPPY)
            .child(el().w(2.0).h(34.0).fill(if on { T::accent() } else { Color::TRANSPARENT }).absolute(0.0, 0.0))
            .child(text(*name).font(if on { STRONG } else { BODY }).color(T::ink()))
            .child(el().grow(1.0))
            .child(text(*file).font(MONO).color(T::faint()))
            .on_press(move |s: &mut State| s.costume = i)
    });
    let costumes = col().fill(T::surface()).border(1.0, T::line_strong()).radius(4.0)
        .child(row().h(38.0).px(12.0).gap(8.0).items_center()
            .child(text("Costumes").font(PANE_TITLE).color(T::ink()))
            .child(text(COSTUMES.len().to_string()).font(MONO).color(T::faint())))
        .child(el().h(1.0).w_full().fill(T::line()))
        .children(rows);
    let facts = col().fill(T::surface()).border(1.0, T::line_strong()).radius(4.0).padding(12.0).gap(6.0)
        .child(text("Renderer").font(PANE_TITLE).color(T::ink()))
        .child(text(format!("hsd-render on Rune's device · wgpu 30")).font(MONO).color(T::muted()))
        .child(text(format!("load {:.0} ms · frame cpu {:.2} ms", s.shown.load_ms, s.shown.cpu_ms)).font(MONO).color(T::muted()))
        .child(text("Drag the stage to orbit").font(BODY).color(T::faint()));
    col().w(320.0).h_full().gap(10.0).child(costumes).child(facts)
}

fn main() {
    let iso = iso_path().unwrap_or_else(|| {
        eprintln!("tgg-next: pass --iso path/to/melee.iso or set TGG_MELEE_ISO");
        std::process::exit(2);
    });
    let viewport = Viewport::new(iso, COSTUMES[0].1);
    rune::App::new(AppSpec::<State> {
        fonts: SANS,
        families: ("Schibsted Grotesk", "DM Mono"),
        poll: Some(poll),
        scenes: &[("orange", |s| s.costume = 1), ("side", |s| s.orbit.yaw = 1.2)],
        ..AppSpec::new("textures.gg", (1280.0, 800.0), init, view)
    })
    .compositor(viewport)
    .run();
}

/// `--iso PATH` (removed from the arguments Rune sees) or `TGG_MELEE_ISO`.
fn iso_path() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--iso") {
        return args.get(i + 1).map(PathBuf::from);
    }
    std::env::var_os("TGG_MELEE_ISO").map(PathBuf::from)
}

/// Each frame: show the compositor's latest timings twice a second, so the
/// view rebuilds for them without rebuilding every frame.
fn poll(s: &mut State, clock: f64, _: &mut rune::Tx) -> bool {
    if clock - s.shown_at < 0.5 && s.shown.size == s.stats.size {
        return false;
    }
    s.shown = s.stats;
    s.shown_at = clock;
    true
}
