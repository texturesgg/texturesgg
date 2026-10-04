//! Pictures of the player's costumes, in their game or their library, and of
//! the models costumes share: each rendered in 3D in its idle pose,
//! off-screen, on a thread of its own with its own GPU device, so the window
//! never waits. Renders are kept on disk by the costume file's SHA-256, so a
//! costume draws once until its file changes.
//!
//! ```text
//! <data dir>/textures.gg/renders/<sha256>-<RENDER_VERSION>-<SIZE>.png
//! ```

use crate::Error;
use crate::ids::SkinId;
use crate::{Game, game_references, load_model};
use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use hsd_render::offscreen::{CAPTURE_FORMAT, Gpu, capture};
use hsd_render::{CameraView, HsdRenderer, Orbit, neutral_preview_lighting};
use melee_dat::{MeleeModel, MeleeSlot, SharedModel};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Change when renders should be redrawn: the renderer, pose, or framing.
const RENDER_VERSION: u32 = 3;
/// Renders are square, this many pixels a side.
pub const SIZE: u32 = 192;
/// How much closer than its camera range a stage is drawn. The range leaves
/// room for fighters far apart, which makes the stage itself small in a
/// square render.
const STAGE_ZOOM: f64 = 0.55;
/// How far above a shared model it is drawn from, in radians.
const SHARED_PITCH: f64 = 0.35;

/// What a render is a picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderKey {
    /// What a slot of the game holds.
    Slot(MeleeSlot),
    /// A skin in the library.
    Skin(SkinId),
    /// A model costumes share, as the game holds it.
    Shared(SharedModel),
}

/// A costume to render, with the game at `iso` for its references.
struct Job {
    iso: PathBuf,
    key: RenderKey,
    /// The slot it fills, or was made for.
    slot: MeleeSlot,
    /// A skin's file on disk; `None` reads the slot from the game.
    path: Option<PathBuf>,
    /// The shared model in the slot's file to draw, rather than the file.
    shared: Option<SharedModel>,
}

/// A finished render: what it is of, and its RGBA pixels, `SIZE` square.
pub struct Rendered {
    pub key: RenderKey,
    pub rgba: Vec<u8>,
}

/// The render thread: send it costumes, receive their pictures.
pub struct Renders {
    jobs: async_channel::Sender<Job>,
    pub rendered: async_channel::Receiver<Rendered>,
}

impl Renders {
    /// Start the render thread.
    pub fn start() -> Self {
        let (jobs, job_queue) = async_channel::unbounded::<Job>();
        let (done, rendered) = async_channel::unbounded();
        std::thread::Builder::new()
            .name("costume renders".into())
            .spawn(move || work(&job_queue, &done))
            .expect("start the render thread");
        Self { jobs, rendered }
    }

    /// Render what `slot` of the game at `iso` holds, from the cache when it
    /// has it.
    pub fn request(&self, iso: &Path, slot: MeleeSlot) {
        let _ = self.jobs.try_send(Job {
            iso: iso.to_owned(),
            key: RenderKey::Slot(slot),
            slot,
            path: None,
            shared: None,
        });
    }

    /// Render the shared model `model` as the game at `iso` holds it.
    pub fn request_shared(&self, iso: &Path, model: SharedModel) {
        let _ = self.jobs.try_send(Job {
            iso: iso.to_owned(),
            key: RenderKey::Shared(model),
            slot: model.slot(),
            path: None,
            shared: Some(model),
        });
    }

    /// Render the skin `id`, its file at `path`, made for `slot`.
    pub fn request_skin(&self, iso: &Path, id: SkinId, path: PathBuf, slot: MeleeSlot) {
        let _ = self.jobs.try_send(Job {
            iso: iso.to_owned(),
            key: RenderKey::Skin(id),
            slot,
            path: Some(path),
            shared: None,
        });
    }

    /// Render the skin `id`, its file at `path`, as `model`, one of the
    /// shared models it holds: a laser skin as its laser.
    pub fn request_skin_shared(&self, iso: &Path, id: SkinId, path: PathBuf, model: SharedModel) {
        let _ = self.jobs.try_send(Job {
            iso: iso.to_owned(),
            key: RenderKey::Skin(id),
            slot: model.slot(),
            path: Some(path),
            shared: Some(model),
        });
    }
}

fn cache_folder() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("textures.gg")
        .join("renders")
}

/// The game the render thread has open, as of its file's last change.
struct OpenGame {
    iso: PathBuf,
    modified: Option<std::time::SystemTime>,
    references: crate::References,
}

/// Render jobs until the app goes away. The GPU device, the open game, and
/// its references live here, off the window's thread.
fn work(jobs: &async_channel::Receiver<Job>, done: &async_channel::Sender<Rendered>) {
    let mut gpu = None;
    let mut game: Option<OpenGame> = None;
    while let Ok(job) = jobs.recv_blocking() {
        match render(&job, &mut gpu, &mut game) {
            Ok(rgba) => {
                if done.send_blocking(Rendered { key: job.key, rgba }).is_err() {
                    return;
                }
            }
            Err(error) => crate::log(&format!("render of {:?} failed: {error}", job.key)),
        }
    }
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
            references: game_references(opened)?,
        });
    }
    let references = &game.as_ref().expect("opened above").references;
    let name = job.slot.file_name();
    let bytes = match &job.path {
        None => references.game().read_slot(job.slot)?,
        Some(path) => std::fs::read(path)?,
    };
    // A shared model is one of its file's: its picture is the file's and
    // its name's.
    let mut key = Sha256::new();
    key.update(&bytes);
    if let Some(shared) = job.shared {
        key.update(shared.name());
    }
    let cached = cache_folder().join(format!("{:x}-{RENDER_VERSION}-{SIZE}.png", key.finalize()));
    if let Ok(image) = image::open(&cached) {
        return Ok(image.to_rgba8().into_raw());
    }
    if gpu.is_none() {
        *gpu = Some(Gpu::request(false)?);
    }
    let gpu = gpu.as_ref().expect("requested above");
    let mut model = match job.shared {
        Some(shared) => {
            MeleeModel::open_shared(&bytes, shared, HsdDrawEvaluationPolicy::GENERIC_HSD)?
        }
        None => load_model(&name, &bytes, Some(references))?.model,
    };
    let geometry = crate::geometry_of(&mut model)?;
    let front = CameraView::Front.orbit();
    let orbit = if job.shared.is_some() {
        // A three-quarter view, so a model as thin as a laser shows its
        // length and one as flat as a shine its face.
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
    let rgba = capture(gpu, &renderer)?.pixels;
    let _ = std::fs::create_dir_all(cache_folder());
    if let Some(image) = image::RgbaImage::from_raw(SIZE, SIZE, rgba.clone()) {
        let _ = image.save(&cached);
    }
    Ok(rgba)
}
