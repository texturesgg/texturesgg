//! Pictures of the player's costumes, in their game or their library: each
//! rendered in 3D in its idle pose,
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
use hsd_render::offscreen::{CAPTURE_FORMAT, Gpu, capture};
use hsd_render::{CameraView, HsdRenderer, Orbit, neutral_preview_lighting};
use melee_dat::MeleeSlot;
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

/// What a render is a picture of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderKey {
    /// What a slot of the game holds.
    Slot(MeleeSlot),
    /// A skin in the library.
    Skin(SkinId),
}

/// A costume to render, with the game at `iso` for its references.
struct Job {
    iso: PathBuf,
    key: RenderKey,
    /// The slot it fills, or was made for.
    slot: MeleeSlot,
    /// A skin's file on disk; `None` reads the slot from the game.
    path: Option<PathBuf>,
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
        });
    }

    /// Render the skin `id`, its file at `path`, made for `slot`.
    pub fn request_skin(&self, iso: &Path, id: SkinId, path: PathBuf, slot: MeleeSlot) {
        let _ = self.jobs.try_send(Job {
            iso: iso.to_owned(),
            key: RenderKey::Skin(id),
            slot,
            path: Some(path),
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
    let cached = cache_folder().join(format!(
        "{:x}-{RENDER_VERSION}-{SIZE}.png",
        Sha256::digest(&bytes)
    ));
    if let Ok(image) = image::open(&cached) {
        return Ok(image.to_rgba8().into_raw());
    }
    if gpu.is_none() {
        *gpu = Some(Gpu::request(false)?);
    }
    let gpu = gpu.as_ref().expect("requested above");
    let mut model = load_model(&name, &bytes, Some(references))?.model;
    let geometry = crate::geometry_of(&mut model)?;
    let front = CameraView::Front.orbit();
    let orbit = if geometry.focus().is_some() {
        Orbit {
            zoom: STAGE_ZOOM,
            ..front
        }
    } else {
        front
    };
    let renderer = HsdRenderer::new(
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
