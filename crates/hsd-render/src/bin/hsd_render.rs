//! Offscreen captures and pixel comparison for the wgpu HSD renderer.
//!
//! ```text
//! hsd-render capture DAT --out DIR [--view ID|all] [--size WxH] [--policy generic-hsd|melee-fighter] [--frame N] [--software]
//! hsd-render compare EXPECTED.png ACTUAL.png [--diff OUT.png]
//! hsd-render regression [--iso ISO] [--out DIR] [--baseline JSON] [--reference DIR] [--update] [--software]
//! hsd-render idle DAT [--iso ISO] --out DIR [--animation N] [--view ID] [--every N] [--size WxH] [--software]
//! hsd-render animations DAT [--iso ISO]
//! hsd-render pick DAT (--at X,Y | --grid N) [--view ID] [--size WxH] [--policy ...] [--software]
//! ```
//!
//! `regression` renders every case of `pixel-baseline.json` and compares a SHA-256 of each capture's
//! pixels with the baseline. It fails on a mismatch, a missing input, an
//! unrecorded view, or an adapter other than the baseline's; `--update` records
//! the current captures and adapter instead. `--reference DIR` also diffs each
//! capture against an earlier run's PNG of the same name. Inputs are the
//! files of a clean Melee NTSC 1.02 disc image, found by SHA-256. A case's `frame`
//! captures a stage that many frames after it loads. A matching hash is
//! deterministic-capture evidence, not a Melee fidelity claim.
//!
//! `idle` plays one full cycle of the catalog Wait1 (or `--animation N`)
//! through the renderer, writing a capture every N ticks, and requires the
//! frame after the loop reset to be byte-identical to the first frame.
//!
//! `animations` lists every animation of the costume's fighter with its move
//! name and length, or why it doesn't bind.
//!
//! `pick` reports which packet and textures draw a pixel of the bind pose
//! (`--at`), or prints an N-column map of the packets across the frame
//! (`--grid`), to check click-to-select against a capture of the same view.
//!
//! `capture` and `regression` evaluate with the generic HSD policy by
//! default; idle always uses the MeleeFighter policy, as the site does.
//!
//! The disc image is `--iso`, else the one `TGG_MELEE_ISO` names. `idle` and
//! `animations` read the fighter files a costume plays with from it.

use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use dat_parser::hsd::source::HsdSource;
use hsd_render::offscreen::{CAPTURE_FORMAT, Gpu, RgbaImage, capture, pick};
use hsd_render::{CameraView, HsdRenderer, PreparedGeometry, neutral_preview_lighting};
use melee_dat::MeleeModel;
use melee_dat::{FighterAttach, MeleeFighterPlayback, MeleeReferenceCatalog, MeleeReferenceStore};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

/// The viewport of the Dolphin captures the baseline was first checked against.
const DEFAULT_SIZE: (u32, u32) = (642, 528);
const DEFAULT_BASELINE: &str = "crates/hsd-render/pixel-baseline.json";
const DEFAULT_REGRESSION_OUT: &str = "target/pixel-regression";
/// 3 added a case's optional `frame`.
const BASELINE_SCHEMA_VERSION: u64 = 3;
/// How a capture is hashed; recorded in the baseline so the rule cannot drift.
const PIXEL_HASH: &str = "sha256 of width and height (u32 little-endian) then RGBA8 rows";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("capture") => run_capture(&args[1..]),
        Some("compare") => run_compare(&args[1..]),
        Some("regression") => run_regression(&args[1..]),
        Some("idle") => run_idle(&args[1..]),
        Some("animations") => run_animations(&args[1..]),
        Some("pick") => run_pick(&args[1..]),
        _ => Err("usage: hsd-render capture|compare|regression|idle|animations|pick ...".into()),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    positional: Vec<String>,
    flags: HashMap<String, String>,
    software: bool,
    update: bool,
}

fn parse_options(args: &[String], valued: &[&str]) -> CliResult<Options> {
    let mut options = Options {
        positional: Vec::new(),
        flags: HashMap::new(),
        software: false,
        update: false,
    };
    let mut iter = args.iter();
    while let Some(argument) = iter.next() {
        if argument == "--software" {
            options.software = true;
        } else if argument == "--update" {
            options.update = true;
        } else if let Some(name) = argument.strip_prefix("--") {
            if !valued.contains(&name) {
                return Err(format!("unknown option {argument}").into());
            }
            let value = iter
                .next()
                .ok_or_else(|| format!("{argument} requires a value"))?;
            options.flags.insert(name.to_owned(), value.clone());
        } else {
            options.positional.push(argument.clone());
        }
    }
    Ok(options)
}

fn parse_size(value: Option<&String>) -> CliResult<(u32, u32)> {
    let Some(value) = value else {
        return Ok(DEFAULT_SIZE);
    };
    let (width, height) = value
        .split_once('x')
        .ok_or("--size must look like 642x528")?;
    let parse = |part: &str| part.parse::<u32>();
    Ok((parse(width)?, parse(height)?))
}

fn parse_views(value: Option<&String>) -> CliResult<Vec<CameraView>> {
    match value.map(String::as_str) {
        None | Some("all") => Ok(CameraView::ALL.to_vec()),
        Some(id) => CameraView::from_id(id)
            .map(|view| vec![view])
            .ok_or_else(|| format!("unknown view {id}").into()),
    }
}

fn run_capture(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["out", "view", "size", "policy", "frame"])?;
    let policy = parse_policy(options.flags.get("policy"))?;
    let [dat] = options.positional.as_slice() else {
        return Err("capture takes one DAT path".into());
    };
    let out = PathBuf::from(options.flags.get("out").ok_or("--out is required")?);
    let size = parse_size(options.flags.get("size"))?;
    let frame = match options.flags.get("frame") {
        Some(frame) => frame.parse().map_err(|error| format!("--frame: {error}"))?,
        None => 0,
    };
    let views = parse_views(options.flags.get("view"))?;
    let gpu = request_gpu(options.software)?;
    let stem = Path::new(dat)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("capture")
        .to_owned();
    std::fs::create_dir_all(&out)?;
    let bytes = std::fs::read(dat).map_err(|error| format!("{dat}: {error}"))?;
    for view in views {
        let image = render_view(&gpu, &bytes, policy, view, size, frame)?;
        let path = out.join(format!("{stem}-{}.png", view.id()));
        write_png(&path, &image)?;
        println!("{}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn run_compare(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["diff"])?;
    let [expected, actual] = options.positional.as_slice() else {
        return Err("compare takes EXPECTED.png ACTUAL.png".into());
    };
    let expected = read_png(Path::new(expected))?;
    let actual = read_png(Path::new(actual))?;
    let metrics = compare(&expected, &actual)?;
    println!("{metrics}");
    if let Some(path) = options.flags.get("diff") {
        write_png(Path::new(path), &diff_image(&expected, &actual))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn run_regression(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["reference", "out", "baseline", "iso", "size"])?;
    let update = options.update;
    let reference = options.flags.get("reference").map(PathBuf::from);
    let out = PathBuf::from(
        options
            .flags
            .get("out")
            .map_or(DEFAULT_REGRESSION_OUT, String::as_str),
    );
    let baseline_path = options
        .flags
        .get("baseline")
        .map_or(DEFAULT_BASELINE, String::as_str);
    let iso = iso_path(&options.flags)?;
    let size = parse_size(options.flags.get("size"))?;
    let mut baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(baseline_path)
            .map_err(|error| format!("{baseline_path}: {error}"))?,
    )
    .map_err(|error| format!("{baseline_path}: {error}"))?;
    if baseline["schema_version"].as_u64() != Some(BASELINE_SCHEMA_VERSION) {
        return Err(
            format!("{baseline_path}: expected schema_version {BASELINE_SCHEMA_VERSION}").into(),
        );
    }
    if baseline["pixel_hash"].as_str() != Some(PIXEL_HASH) {
        return Err(format!("{baseline_path}: pixel_hash must be \"{PIXEL_HASH}\"").into());
    }
    let recorded_size = baseline["size"]
        .as_array()
        .and_then(|size| Some((size.first()?.as_u64()?, size.get(1)?.as_u64()?)));
    if recorded_size != Some((u64::from(size.0), u64::from(size.1))) && !update {
        return Err(format!(
            "{baseline_path} was recorded at another size; pass --size or --update"
        )
        .into());
    }
    let wanted: Vec<String> = baseline["cases"]
        .as_array()
        .ok_or("baseline has no cases")?
        .iter()
        .filter_map(|case| case["input_sha256"].as_str().map(str::to_owned))
        .collect();
    let wanted_refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    let found = index_inputs(&iso, &wanted_refs)?;
    std::fs::create_dir_all(&out)?;
    let gpu = request_gpu(options.software)?;
    let info = gpu.adapter.get_info();
    let adapter = format!("{} ({:?})", info.name, info.backend);
    println!("adapter: {adapter}, driver {}", info.driver);
    let recorded_adapter = baseline["adapter"].as_str().unwrap_or_default().to_owned();
    if recorded_adapter != adapter && !update {
        return Err(format!(
            "baseline hashes were recorded on {recorded_adapter:?}, not {adapter:?}; \
             rerun with the same adapter (--software selects lavapipe) or --update"
        )
        .into());
    }

    let mut report = Vec::new();
    let (mut mismatched, mut missing, mut unrecorded, mut failed) = (0, 0, 0, 0);
    let cases = baseline["cases"]
        .as_array_mut()
        .ok_or("baseline has no cases")?;
    for case in cases {
        let name = case["name"]
            .as_str()
            .ok_or("case without a name")?
            .to_owned();
        let hash = case["input_sha256"]
            .as_str()
            .ok_or("case without a hash")?
            .to_owned();
        let Some(bytes) = found.get(&hash) else {
            println!("missing  {name}");
            missing += 1;
            continue;
        };
        // A stage is captured this many frames after it loads.
        let frame = match &case["frame"] {
            serde_json::Value::Null => 0,
            frame => frame
                .as_u64()
                .and_then(|frame| u32::try_from(frame).ok())
                .ok_or_else(|| format!("{name}: frame must be a small whole number"))?,
        };
        let views = case["views"].as_object_mut().ok_or("case without views")?;
        for (view_id, expected) in views.iter_mut() {
            let view =
                CameraView::from_id(view_id).ok_or_else(|| format!("unknown view {view_id}"))?;
            let label = format!("{name}-{view_id}");
            let image = match render_view(
                &gpu,
                bytes,
                HsdDrawEvaluationPolicy::GENERIC_HSD,
                view,
                size,
                frame,
            ) {
                Ok(image) => image,
                Err(error) => {
                    println!("failed   {label}: {error}");
                    report.push(format!("{label}\tfailed\t{error}"));
                    failed += 1;
                    continue;
                }
            };
            write_png(&out.join(format!("{label}.png")), &image)?;
            let actual = pixel_hash(&image);
            let status = match expected.as_str() {
                _ if update => "recorded",
                Some(expected) if expected == actual => "match",
                Some(_) => {
                    mismatched += 1;
                    "mismatch"
                }
                None => {
                    unrecorded += 1;
                    "new"
                }
            };
            if update {
                *expected = serde_json::Value::String(actual.clone());
            }
            let mut line = format!("{status:<9}{label}");
            if let Some(reference) = &reference
                && let Ok(previous) = read_png(&reference.join(format!("{label}.png")))
            {
                let metrics = compare(&previous, &image)?;
                write_png(
                    &out.join(format!("{label}-diff.png")),
                    &diff_image(&previous, &image),
                )?;
                line = format!("{line:<50} {metrics}");
            }
            println!("{line}");
            report.push(format!("{label}\t{status}\t{actual}"));
        }
    }
    std::fs::write(out.join("report.tsv"), report.join("\n") + "\n")?;
    println!("captures: {}", out.display());
    if update {
        if missing > 0 || failed > 0 {
            return Err(format!(
                "not updating: {missing} case(s) missing inputs and {failed} capture(s) failed"
            )
            .into());
        }
        baseline["adapter"] = serde_json::Value::String(adapter);
        baseline["size"] = serde_json::json!([size.0, size.1]);
        std::fs::write(
            baseline_path,
            serde_json::to_string_pretty(&baseline)? + "\n",
        )
        .map_err(|error| format!("{baseline_path}: {error}"))?;
        println!("updated {baseline_path}");
        return Ok(ExitCode::SUCCESS);
    }
    if mismatched + missing + unrecorded + failed > 0 {
        println!(
            "{mismatched} mismatched, {unrecorded} unrecorded, {missing} missing, {failed} failed"
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// A capture's identity: dimensions plus exact pixels, independent of PNG encoding.
fn pixel_hash(image: &RgbaImage) -> String {
    let mut digest = Sha256::new();
    digest.update(image.width.to_le_bytes());
    digest.update(image.height.to_le_bytes());
    digest.update(&image.pixels);
    hex(&digest.finalize())
}

fn parse_policy(value: Option<&String>) -> CliResult<HsdDrawEvaluationPolicy> {
    match value.map(String::as_str) {
        None | Some("generic-hsd") => Ok(HsdDrawEvaluationPolicy::GENERIC_HSD),
        Some("melee-fighter") => Ok(HsdDrawEvaluationPolicy::MELEE_FIGHTER),
        Some(other) => {
            Err(format!("--policy must be generic-hsd or melee-fighter, not {other}").into())
        }
    }
}

fn run_pick(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["at", "grid", "view", "size", "policy"])?;
    let policy = parse_policy(options.flags.get("policy"))?;
    let [dat] = options.positional.as_slice() else {
        return Err("pick takes one DAT path".into());
    };
    let size = parse_size(options.flags.get("size"))?;
    let view = match options.flags.get("view") {
        Some(id) => CameraView::from_id(id).ok_or_else(|| format!("unknown view {id}"))?,
        None => CameraView::Front,
    };
    let gpu = request_gpu(options.software)?;
    let bytes = std::fs::read(dat).map_err(|error| format!("{dat}: {error}"))?;
    let mut source = HsdSource::from_dat(&bytes, policy)?;
    let geometry = PreparedGeometry::bind_pose(&mut source)?;
    let mut renderer = HsdRenderer::new(
        &gpu.device,
        &gpu.queue,
        CAPTURE_FORMAT,
        geometry,
        neutral_preview_lighting(),
        size,
        view.orbit(),
    )?;
    let describe = |renderer: &HsdRenderer, packet: usize| {
        let polygon = renderer.geometry().packets[packet].polygon_source_id;
        let textures: Vec<String> = renderer
            .packet_textures(packet)
            .iter()
            .map(|texture| {
                format!(
                    "stage {} scene {:?}{}",
                    texture.stage,
                    texture.scene_textures,
                    if texture.reflection {
                        " (reflection)"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        format!(
            "packet {packet} (PObj {polygon:#x}): {}",
            textures.join(", ")
        )
    };
    if let Some(at) = options.flags.get("at") {
        let (x, y) = at.split_once(',').ok_or("--at must look like 320,200")?;
        let parse = |part: &str| part.parse::<u32>();
        let started = std::time::Instant::now();
        let picked = pick(&gpu, &mut renderer, parse(x)?, parse(y)?)?;
        let elapsed = started.elapsed();
        match picked {
            Some(packet) => println!("{}", describe(&renderer, packet)),
            None => println!("background"),
        }
        println!(
            "pick took {:.2} ms (first pick builds the pick pipelines)",
            elapsed.as_secs_f64() * 1000.0
        );
        let started = std::time::Instant::now();
        pick(&gpu, &mut renderer, parse(x)?, parse(y)?)?;
        println!(
            "repeat pick took {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        return Ok(ExitCode::SUCCESS);
    }
    let columns: u32 = options
        .flags
        .get("grid")
        .ok_or("pick needs --at X,Y or --grid N")?
        .parse()
        .map_err(|error: std::num::ParseIntError| error.to_string())?;
    // Terminal cells are about twice as tall as wide.
    let rows = (columns * size.1 / size.0 / 2).max(1);
    let mut seen = std::collections::BTreeSet::new();
    const GLYPHS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for row in 0..rows {
        let mut line = String::new();
        for column in 0..columns {
            let x = (column * 2 + 1) * size.0 / (columns * 2);
            let y = (row * 2 + 1) * size.1 / (rows * 2);
            match pick(&gpu, &mut renderer, x, y)? {
                Some(packet) => {
                    seen.insert(packet);
                    line.push(GLYPHS[packet % GLYPHS.len()] as char);
                }
                None => line.push('.'),
            }
        }
        println!("{line}");
    }
    for packet in seen {
        println!(
            "{} {}",
            GLYPHS[packet % GLYPHS.len()] as char,
            describe(&renderer, packet)
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn request_gpu(software: bool) -> CliResult<Gpu> {
    Ok(Gpu::request(software)?)
}

fn render_view(
    gpu: &Gpu,
    bytes: &[u8],
    policy: HsdDrawEvaluationPolicy,
    view: CameraView,
    size: (u32, u32),
    frame: u32,
) -> CliResult<RgbaImage> {
    // A stage is drawn as it loads, `frame` ticks in; anything else in its
    // bind pose.
    let mut model = MeleeModel::open(bytes, policy)?;
    for _ in 0..frame {
        model.advance()?;
    }
    let focus = model.focus();
    let (scene, work) = model.evaluate()?;
    let geometry = PreparedGeometry::new(scene, work)?.with_focus(focus);
    let renderer = HsdRenderer::new(
        &gpu.device,
        &gpu.queue,
        CAPTURE_FORMAT,
        geometry,
        neutral_preview_lighting(),
        size,
        view.orbit(),
    )?;
    Ok(capture(gpu, &renderer, size.0, size.1)?)
}

/// The disc image to read: `--iso`, else `TGG_MELEE_ISO`.
fn iso_path(flags: &HashMap<String, String>) -> CliResult<PathBuf> {
    flags
        .get("iso")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TGG_MELEE_ISO").map(PathBuf::from))
        .ok_or_else(|| "name a clean Melee NTSC 1.02 disc image with --iso or TGG_MELEE_ISO".into())
}

/// The wanted files of the disc image at `iso`, by SHA-256.
fn index_inputs(iso: &Path, wanted: &[&str]) -> CliResult<HashMap<String, Vec<u8>>> {
    let mut disc =
        gc_iso::Disc::open(iso).map_err(|error| format!("{}: {error}", iso.display()))?;
    let names: Vec<String> = disc
        .files()
        .iter()
        .filter(|file| !file.is_dir && file.name.to_ascii_lowercase().ends_with(".dat"))
        .map(|file| file.name.clone())
        .collect();
    let mut found = HashMap::new();
    for name in names {
        let bytes = disc.read(&name)?;
        let hash = hex(&Sha256::digest(&bytes));
        if wanted.contains(&hash.as_str()) {
            found.entry(hash).or_insert(bytes);
        }
    }
    Ok(found)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Metrics {
    pixels: usize,
    identical: usize,
    within_2: usize,
    over_16: usize,
    over_64: usize,
    mean_abs: f64,
    max_delta: u8,
}

impl std::fmt::Display for Metrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let percent = |count: usize| 100.0 * count as f64 / self.pixels as f64;
        write!(
            formatter,
            "identical {:6.2}%  within±2 {:6.2}%  >16 {:6.3}%  >64 {:6.3}%  mean|Δ| {:.3}  max {}",
            percent(self.identical),
            percent(self.within_2),
            percent(self.over_16),
            percent(self.over_64),
            self.mean_abs,
            self.max_delta
        )
    }
}

fn compare(expected: &RgbaImage, actual: &RgbaImage) -> CliResult<Metrics> {
    if (expected.width, expected.height) != (actual.width, actual.height) {
        return Err(format!(
            "size mismatch: {}x{} vs {}x{}",
            expected.width, expected.height, actual.width, actual.height
        )
        .into());
    }
    let mut metrics = Metrics {
        pixels: expected.pixels.len() / 4,
        identical: 0,
        within_2: 0,
        over_16: 0,
        over_64: 0,
        mean_abs: 0.0,
        max_delta: 0,
    };
    let mut total = 0u64;
    for (left, right) in expected
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.pixels.as_chunks::<4>().0)
    {
        let delta = (0..3)
            .map(|channel| left[channel].abs_diff(right[channel]))
            .max()
            .unwrap_or(0);
        total += (0..3)
            .map(|channel| u64::from(left[channel].abs_diff(right[channel])))
            .sum::<u64>();
        metrics.identical += usize::from(delta == 0);
        metrics.within_2 += usize::from(delta <= 2);
        metrics.over_16 += usize::from(delta > 16);
        metrics.over_64 += usize::from(delta > 64);
        metrics.max_delta = metrics.max_delta.max(delta);
    }
    metrics.mean_abs = total as f64 / (metrics.pixels * 3) as f64;
    Ok(metrics)
}

/// Grayscale expected image with differing pixels painted by magnitude:
/// yellow for small (≤16) and red for large differences.
fn diff_image(expected: &RgbaImage, actual: &RgbaImage) -> RgbaImage {
    let mut pixels = Vec::with_capacity(expected.pixels.len());
    for (left, right) in expected
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.pixels.as_chunks::<4>().0)
    {
        let delta = (0..3)
            .map(|channel| left[channel].abs_diff(right[channel]))
            .max()
            .unwrap_or(0);
        let gray = ((u16::from(left[0]) + u16::from(left[1]) + u16::from(left[2])) / 12) as u8;
        pixels.extend_from_slice(&match delta {
            0..=2 => [gray, gray, gray, 255],
            3..=16 => [255, 220, 0, 255],
            _ => [255, 0, 0, 255],
        });
    }
    RgbaImage {
        width: expected.width,
        height: expected.height,
        pixels,
    }
}

fn write_png(path: &Path, image: &RgbaImage) -> CliResult<()> {
    let file =
        std::fs::File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    Ok(writer.write_image_data(&image.pixels)?)
}

fn read_png(path: &Path) -> CliResult<RgbaImage> {
    let file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG is too large")?];
    let frame = reader.next_frame(&mut buffer)?;
    let data = &buffer[..frame.buffer_size()];
    let pixels = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        other => {
            return Err(format!("{}: unsupported PNG color type {other:?}", path.display()).into());
        }
    };
    Ok(RgbaImage {
        width: frame.width,
        height: frame.height,
        pixels,
    })
}

/// Attach the catalog playback to the costume at `dat`, with the fighter
/// files it plays with read from the disc image at `iso`.
fn attach_playback(dat: &str, iso: &Path) -> CliResult<MeleeFighterPlayback> {
    let catalog = MeleeReferenceCatalog::checked_in();
    let bytes = std::fs::read(dat).map_err(|error| format!("{dat}: {error}"))?;
    let source = HsdSource::from_dat(&bytes, HsdDrawEvaluationPolicy::MELEE_FIGHTER)?;
    let mut disc =
        gc_iso::Disc::open(iso).map_err(|error| format!("{}: {error}", iso.display()))?;
    let store =
        MeleeReferenceStore::for_costume(catalog, &source.scene, |name| disc.read(name).ok())
            .ok_or("no catalog idle profile admits this costume")?;
    match MeleeFighterPlayback::attach(source, catalog, &store) {
        FighterAttach::Attached(playback) => Ok(*playback),
        FighterAttach::Unrecognized(_) => Err("no catalog idle profile admits this costume".into()),
        FighterAttach::Failed { error, .. } => Err(error.into()),
    }
}

fn run_animations(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["iso"])?;
    let [dat] = options.positional.as_slice() else {
        return Err("animations takes one costume DAT path".into());
    };
    let iso = iso_path(&options.flags)?;
    let mut playback = attach_playback(dat, &iso)?;
    let animations = playback.animations().to_vec();
    let (mut bound, mut named) = (0, 0);
    for animation in &animations {
        let action = animation.action.as_deref().unwrap_or("-");
        let name = animation.name.as_deref().unwrap_or("");
        named += usize::from(animation.name.is_some());
        match playback
            .check(animation.index)
            .and_then(|()| playback.play(animation.index))
        {
            Ok(()) => {
                bound += 1;
                println!(
                    "{:4} {action:28} {name:40} {} frames",
                    animation.index,
                    playback.end_frame()
                );
            }
            Err(error) => println!(
                "{:4} {action:28} {name:40} refused: {error}",
                animation.index
            ),
        }
    }
    println!(
        "{bound} of {} animations bind; {named} have move names",
        animations.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn run_idle(args: &[String]) -> CliResult<ExitCode> {
    let options = parse_options(args, &["iso", "out", "view", "every", "size", "animation"])?;
    let [dat] = options.positional.as_slice() else {
        return Err("idle takes one costume DAT path".into());
    };
    let iso = iso_path(&options.flags)?;
    let out = PathBuf::from(options.flags.get("out").ok_or("--out is required")?);
    let size = parse_size(options.flags.get("size"))?;
    let view = match options.flags.get("view") {
        Some(id) => CameraView::from_id(id).ok_or_else(|| format!("unknown view {id}"))?,
        None => CameraView::Front,
    };
    let every: u64 = options
        .flags
        .get("every")
        .map_or(Ok(10), |value| value.parse())
        .map_err(|error| format!("--every: {error}"))?;
    let mut playback = attach_playback(dat, &iso)?;
    if let Some(index) = options.flags.get("animation") {
        let index = index
            .parse()
            .map_err(|error| format!("--animation: {error}"))?;
        playback.play(index)?;
    }
    let gpu = request_gpu(options.software)?;
    let stem = Path::new(dat)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("idle")
        .to_owned();
    std::fs::create_dir_all(&out)?;

    let (scene, work) = playback.evaluate()?;
    let geometry = PreparedGeometry::new(scene, work)?;
    let mut renderer = HsdRenderer::new(
        &gpu.device,
        &gpu.queue,
        CAPTURE_FORMAT,
        geometry,
        neutral_preview_lighting(),
        size,
        view.orbit(),
    )?;
    let first = capture(&gpu, &renderer, size.0, size.1)?;
    write_png(&out.join(format!("{stem}-{}-t0000.png", view.id())), &first)?;

    let started = std::time::Instant::now();
    let mut frame_time = std::time::Duration::ZERO;
    let mut frames = 0u32;
    while playback.loops() == 0 {
        playback.advance()?;
        let frame_started = std::time::Instant::now();
        let (scene, work) = playback.evaluate()?;
        renderer.update_draw_work(&gpu.queue, scene, work)?;
        frame_time += frame_started.elapsed();
        frames += 1;
        if playback.loops() > 0 {
            let reset = capture(&gpu, &renderer, size.0, size.1)?;
            let exact = reset.pixels == first.pixels;
            println!(
                "{}: {} ticks per cycle; reset frame {} the first frame",
                playback.label(),
                playback.tick(),
                if exact { "matches" } else { "DIFFERS from" }
            );
            if !exact {
                write_png(&out.join(format!("{stem}-{}-reset.png", view.id())), &reset)?;
                return Ok(ExitCode::FAILURE);
            }
        } else if playback.tick() % every == 0 {
            let image = capture(&gpu, &renderer, size.0, size.1)?;
            write_png(
                &out.join(format!("{stem}-{}-t{:04}.png", view.id(), playback.tick())),
                &image,
            )?;
        }
    }
    println!(
        "evaluate + upload: {:.3} ms/frame over {frames} frames ({:.1} s total with captures)",
        frame_time.as_secs_f64() * 1000.0 / f64::from(frames.max(1)),
        started.elapsed().as_secs_f64()
    );
    println!("captures: {}", out.display());
    Ok(ExitCode::SUCCESS)
}
