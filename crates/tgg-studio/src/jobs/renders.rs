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

use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use hsd_render::offscreen::{CAPTURE_FORMAT, Gpu, capture, capture_clear};
use hsd_render::{CameraView, HsdRenderer, Orbit, PreparedGeometry, neutral_preview_lighting};
use melee_dat::{MeleeModel, MeleeSlot};
use rune::Pixels;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use tgg_app::ids::SkinId;
use tgg_app::{Game, References};

/// Change when renders should be redrawn: the renderer, pose, or framing.
/// tgg-editor's, so the two apps share their cache.
const RENDER_VERSION: u32 = 4;
/// Renders are square, this many pixels a side.
pub const SIZE: u32 = 192;
/// How far above a shared model it is drawn from, in radians.
const SHARED_PITCH: f64 = 0.35;
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
    /// What a slot holds, drawn with nothing behind it (Play's fighter).
    Fighter(MeleeSlot),
    /// A skin in the library.
    Skin(SkinId),
}

/// A picture to draw, with the game at `iso` for its references.
struct Job {
    /// What it's the picture of.
    key: RenderKey,
    iso: PathBuf,
    /// The slot it fills, or was made for.
    slot: MeleeSlot,
    /// A skin's file; `None` reads the slot from the game.
    file: Option<PathBuf>,
    /// Clear the background, so the picture sits on whatever is under it.
    clear: bool,
    pixels: Pixels,
}

/// The render worker, and the pictures asked of it.
#[derive(Default)]
pub struct Renders {
    jobs: Option<mpsc::Sender<Job>>,
    pictures: HashMap<RenderKey, Pixels>,
    /// The pictures the worker has drawn (or couldn't draw), shared with it.
    drawn: Arc<Mutex<HashSet<RenderKey>>>,
}

impl Renders {
    /// The picture of `key`, once asked for: empty until the worker fills it.
    pub fn get(&self, key: RenderKey) -> Option<&Pixels> {
        self.pictures.get(&key)
    }

    /// Ask for a picture of what `slot` of the game at `iso` holds, unless
    /// it's been asked for already. The worker starts on the first request.
    pub fn request(&mut self, iso: &Path, slot: MeleeSlot) {
        self.ask(Job {
            key: RenderKey::Slot(slot),
            iso: iso.to_owned(),
            slot,
            file: None,
            clear: false,
            pixels: Pixels::new(),
        });
    }

    /// [`request`](Self::request) with nothing behind the model, for a
    /// picture drawn over the page's own surface.
    pub fn request_fighter(&mut self, iso: &Path, slot: MeleeSlot) {
        self.ask(Job {
            key: RenderKey::Fighter(slot),
            iso: iso.to_owned(),
            slot,
            file: None,
            clear: true,
            pixels: Pixels::new(),
        });
    }

    /// Ask for a picture of the skin `id`, its file at `file`, made for
    /// `slot`, drawn with the game at `iso`'s references.
    pub fn request_skin(&mut self, iso: &Path, id: SkinId, file: PathBuf, slot: MeleeSlot) {
        self.ask(Job {
            key: RenderKey::Skin(id),
            iso: iso.to_owned(),
            slot,
            file: Some(file),
            clear: false,
            pixels: Pixels::new(),
        });
    }

    /// Whether the picture of `key` has been drawn (or its drawing failed).
    /// Scenes wait on it, so their pictures are in before the shot.
    pub fn drawn(&self, key: RenderKey) -> bool {
        self.drawn.lock().is_ok_and(|drawn| drawn.contains(&key))
    }

    fn ask(&mut self, job: Job) {
        let key = job.key;
        if self.pictures.contains_key(&key) {
            return;
        }
        self.pictures.insert(key, job.pixels.clone());
        let drawn = self.drawn.clone();
        let jobs = self.jobs.get_or_insert_with(|| start(drawn));
        let _ = jobs.send(job);
    }

    /// Forget the picture of `key` (its file changed), so the next request
    /// draws it again.
    pub fn forget(&mut self, key: RenderKey) {
        self.pictures.remove(&key);
        if let Ok(mut drawn) = self.drawn.lock() {
            drawn.remove(&key);
        }
    }

    /// Forget every picture: after an install changed what slots hold, or
    /// when the game changes.
    pub fn clear(&mut self) {
        self.pictures.clear();
        if let Ok(mut drawn) = self.drawn.lock() {
            drawn.clear();
        }
    }
}

/// Start the worker thread.
fn start(drawn: Arc<Mutex<HashSet<RenderKey>>>) -> mpsc::Sender<Job> {
    let (jobs, queue) = mpsc::channel();
    std::thread::Builder::new()
        .name("costume renders".into())
        .spawn(move || work(&queue, &drawn))
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
fn work(queue: &mpsc::Receiver<Job>, drawn: &Mutex<HashSet<RenderKey>>) {
    let mut gpu = None;
    let mut game: Option<OpenGame> = None;
    while let Ok(job) = queue.recv() {
        match render(&job, &mut gpu, &mut game) {
            Ok(rgba) => job.pixels.set(SIZE, SIZE, rgba),
            Err(error) => tgg_app::log(&format!(
                "render of {} failed: {error}",
                job.slot.file_name()
            )),
        }
        if let Ok(mut drawn) = drawn.lock() {
            drawn.insert(job.key);
        }
    }
}

fn cache_folder() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("textures.gg")
        .join("renders")
}

fn render(job: &Job, gpu: &mut Option<Gpu>, game: &mut Option<OpenGame>) -> Result<Vec<u8>, Error> {
    // An install may have moved files on the disc: reopen it when it changed.
    let modified = std::fs::metadata(&job.iso)
        .and_then(|metadata| metadata.modified())
        .ok();
    let stale = game
        .as_ref()
        .is_none_or(|open| open.iso != job.iso || open.modified != modified);
    if stale {
        let opened = Game::open(&job.iso)?;
        *game = Some(OpenGame {
            iso: job.iso.clone(),
            modified,
            references: tgg_app::game_references(opened)?,
        });
    }
    let references = &game.as_ref().expect("opened above").references;
    let bytes = match &job.file {
        None => references.game().read_slot(job.slot)?,
        Some(file) => std::fs::read(file)?,
    };
    // A skin for a file costumes share draws as the first model it changes:
    // a laser skin as its laser. Its picture is the file's and the model's.
    let whole = matches!(job.slot, MeleeSlot::Costume { .. } | MeleeSlot::Stage(_));
    let shared = (!whole)
        .then(|| {
            tgg_app::costumes::changed_models(job.slot, &bytes)
                .into_iter()
                .next()
        })
        .flatten();
    if !whole && shared.is_none() {
        return Err("it changes no model the app can draw".into());
    }
    let mut key = Sha256::new();
    key.update(&bytes);
    if let Some(model) = shared {
        key.update(model.name());
    }
    let behind = if job.clear { "-clear" } else { "" };
    let cached = cache_folder().join(format!(
        "{:x}-{RENDER_VERSION}-{SIZE}{behind}.png",
        key.finalize()
    ));
    if let Ok(image) = image::open(&cached) {
        return Ok(image.to_rgba8().into_raw());
    }
    let gpu = match gpu {
        Some(gpu) => gpu,
        None => gpu.insert(Gpu::request(false)?),
    };
    let mut model = match shared {
        Some(shared) => {
            MeleeModel::open_shared(&bytes, shared, HsdDrawEvaluationPolicy::GENERIC_HSD)?
        }
        None => tgg_app::load_model(&job.slot.file_name(), &bytes, Some(references))?.model,
    };
    let focus = model.focus();
    let (scene, work) = model.evaluate()?;
    let geometry = PreparedGeometry::new(scene, work)?.with_focus(focus);
    let front = CameraView::Front.orbit();
    let orbit = if shared.is_some() {
        // A three-quarter view, so a model as thin as a laser shows its length
        // and one as flat as a shine its face.
        Orbit {
            yaw: std::f64::consts::FRAC_PI_4,
            pitch: SHARED_PITCH,
            ..front
        }
    } else if geometry.focus().is_some() {
        Orbit {
            zoom: STAGE_ZOOM,
            ..front
        }
    } else {
        front
    };
    let (renderer, _) = HsdRenderer::with_model(
        &gpu.device,
        &gpu.queue,
        CAPTURE_FORMAT,
        geometry,
        neutral_preview_lighting(),
        (SIZE, SIZE),
        orbit,
    )?;
    let rgba = if job.clear {
        capture_clear(gpu, &renderer)?.pixels
    } else {
        capture(gpu, &renderer)?.pixels
    };
    let _ = std::fs::create_dir_all(cache_folder());
    if let Some(image) = image::RgbaImage::from_raw(SIZE, SIZE, rgba.clone()) {
        let _ = image.save(&cached);
    }
    Ok(rgba)
}
