//! Pictures of the player's costumes and stages: each rendered in 3D in its
//! idle pose, off-screen, on a thread of its own with its own GPU device, so
//! the window never waits. The worker fills each picture's `Pixels` itself,
//! which redraws whatever shows it. Renders are kept on disk by the file's
//! SHA-256, where tgg-editor keeps them too, so a costume draws once until
//! its file changes.
//!
//! ```text
//! <data dir>/textures.gg/renders/<sha256>-<RENDER_VERSION>-<SIZE>.png
//! ```

use hsd_render::offscreen::{CAPTURE_FORMAT, Gpu, capture};
use hsd_render::{CameraView, HsdRenderer, Orbit, PreparedGeometry, neutral_preview_lighting};
use melee_dat::MeleeSlot;
use rune::Pixels;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tgg_app::{Game, References};

/// Change when renders should be redrawn: the renderer, pose, or framing.
/// tgg-editor's, so the two apps share their cache.
const RENDER_VERSION: u32 = 3;
/// Renders are square, this many pixels a side.
pub const SIZE: u32 = 192;
/// How much closer than its camera range a stage is drawn. The range leaves
/// room for fighters far apart, which makes the stage itself small in a
/// square render.
const STAGE_ZOOM: f64 = 0.55;

type Error = Box<dyn std::error::Error>;

/// What a render is a picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderKey {
    /// What a slot of the game holds.
    Slot(MeleeSlot),
}

/// A picture to draw, from the game at `iso`.
struct Job {
    iso: PathBuf,
    slot: MeleeSlot,
    pixels: Pixels,
}

/// The render worker, and the pictures asked of it.
#[derive(Default)]
pub struct Renders {
    jobs: Option<mpsc::Sender<Job>>,
    pictures: HashMap<RenderKey, Pixels>,
}

impl Renders {
    /// The picture of `key`, once asked for: empty until the worker fills it.
    pub fn get(&self, key: RenderKey) -> Option<&Pixels> {
        self.pictures.get(&key)
    }

    /// Ask for a picture of what `slot` of the game at `iso` holds, unless
    /// it's been asked for already. The worker starts on the first request.
    pub fn request(&mut self, iso: &Path, slot: MeleeSlot) {
        let key = RenderKey::Slot(slot);
        if self.pictures.contains_key(&key) {
            return;
        }
        let pixels = Pixels::new();
        self.pictures.insert(key, pixels.clone());
        let jobs = self.jobs.get_or_insert_with(start);
        let _ = jobs.send(Job { iso: iso.to_owned(), slot, pixels });
    }

    /// Forget every picture: after an install changed what slots hold, or
    /// when the game changes.
    pub fn clear(&mut self) {
        self.pictures.clear();
    }
}

/// Start the worker thread.
fn start() -> mpsc::Sender<Job> {
    let (jobs, queue) = mpsc::channel();
    std::thread::Builder::new()
        .name("costume renders".into())
        .spawn(move || work(&queue))
        .expect("start the render thread");
    jobs
}

/// The game the worker has open, as of its file's last change.
struct OpenGame {
    iso: PathBuf,
    modified: Option<std::time::SystemTime>,
    references: References,
}

/// Render jobs until the app goes away. The GPU device, the open game and
/// its references live here, off the window's thread.
fn work(queue: &mpsc::Receiver<Job>) {
    let mut gpu = None;
    let mut game: Option<OpenGame> = None;
    while let Ok(job) = queue.recv() {
        match render(&job, &mut gpu, &mut game) {
            Ok(rgba) => job.pixels.set(SIZE, SIZE, rgba),
            Err(error) => tgg_app::log(&format!("render of {} failed: {error}", job.slot.file_name())),
        }
    }
}

fn cache_folder() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("textures.gg").join("renders")
}

fn render(job: &Job, gpu: &mut Option<Gpu>, game: &mut Option<OpenGame>) -> Result<Vec<u8>, Error> {
    // An install may have moved files on the disc: reopen it when it changed.
    let modified = std::fs::metadata(&job.iso).and_then(|metadata| metadata.modified()).ok();
    let stale = game.as_ref().is_none_or(|open| open.iso != job.iso || open.modified != modified);
    if stale {
        let opened = Game::open(&job.iso)?;
        *game = Some(OpenGame { iso: job.iso.clone(), modified, references: tgg_app::game_references(opened)? });
    }
    let references = &game.as_ref().expect("opened above").references;
    let bytes = references.game().read_slot(job.slot)?;
    let cached = cache_folder().join(format!("{:x}-{RENDER_VERSION}-{SIZE}.png", Sha256::digest(&bytes)));
    if let Ok(image) = image::open(&cached) {
        return Ok(image.to_rgba8().into_raw());
    }
    let gpu = match gpu {
        Some(gpu) => gpu,
        None => gpu.insert(Gpu::request(false)?),
    };
    let mut model = tgg_app::load_model(&job.slot.file_name(), &bytes, Some(references))?.model;
    let focus = model.focus();
    let (scene, work) = model.evaluate()?;
    let geometry = PreparedGeometry::new(scene, work)?.with_focus(focus);
    let front = CameraView::Front.orbit();
    let orbit = if geometry.focus().is_some() { Orbit { zoom: STAGE_ZOOM, ..front } } else { front };
    let (renderer, _) = HsdRenderer::with_model(
        &gpu.device,
        &gpu.queue,
        CAPTURE_FORMAT,
        geometry,
        neutral_preview_lighting(),
        (SIZE, SIZE),
        orbit,
    )?;
    let rgba = capture(gpu, &renderer)?.pixels;
    let _ = std::fs::create_dir_all(cache_folder());
    if let Some(image) = image::RgbaImage::from_raw(SIZE, SIZE, rgba.clone()) {
        let _ = image.save(&cached);
    }
    Ok(rgba)
}
