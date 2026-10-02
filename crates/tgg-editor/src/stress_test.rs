//! The stress test: every costume in the player's game animated at once, with
//! the window's frame rate and what drawing them costs. Tucked in Settings;
//! it shows how far the renderer goes, not something players need.
//!
//! Each costume gets its own viewport, exactly as Your game draws one: no
//! batching or sharing yet, so the numbers are the naive baseline.

use crate::costumes::{roster, slot_label};
use crate::viewport::Viewport;
use crate::{References, load_model, log};
use gpui::{
    AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, StatefulInteractiveElement, Styled, Task, Window, canvas, div, px,
};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tgg_ui::tokens::{font, space, text};
use tgg_ui::{Theme, Tooltip, rem};

/// A cell's height over its width: fighters stand taller than wide.
const ASPECT: f32 = 1.25;

struct Cell {
    label: String,
    viewport: Entity<Viewport>,
}

pub(crate) struct StressTest {
    references: Rc<References>,
    /// Costume files still to load, one between frames.
    queue: VecDeque<String>,
    total: usize,
    cells: Vec<Cell>,
    failed: usize,
    started: Instant,
    loaded_in: Option<Duration>,
    /// When the window's recent frames started, to show its frame rate.
    frames: VecDeque<Instant>,
    /// The grid's area, measured as it paints.
    area: Option<Bounds<Pixels>>,
    last_report: Option<Instant>,
    _loading: Task<()>,
}

impl StressTest {
    pub fn new(references: Rc<References>, cx: &mut Context<Self>) -> Self {
        let files: VecDeque<String> =
            roster(references.game().file_names().iter().map(String::as_str))
                .into_iter()
                .flat_map(|fighter| fighter.costumes.into_iter().map(|costume| costume.file))
                .collect();
        // Load between frames, so the grid fills in while the window stays
        // responsive.
        let loading = cx.spawn(async move |this, cx| {
            while this
                .update(cx, |this, cx| this.load_next(cx))
                .unwrap_or(false)
            {
                cx.background_executor().timer(Duration::ZERO).await;
            }
        });
        Self {
            references,
            total: files.len(),
            queue: files,
            cells: Vec::new(),
            failed: 0,
            started: Instant::now(),
            loaded_in: None,
            frames: VecDeque::new(),
            area: None,
            last_report: None,
            _loading: loading,
        }
    }

    /// Load the next costume into a viewport of its own; whether more wait.
    fn load_next(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(file) = self.queue.pop_front() else {
            return false;
        };
        let loaded = self
            .references
            .game()
            .read(&file)
            .and_then(|bytes| load_model(&file, &bytes, Some(&self.references)));
        match loaded {
            Ok(loaded) => {
                let viewport = cx.new(|cx| Viewport::new(loaded.model, cx.focus_handle()).quiet());
                self.cells.push(Cell {
                    label: slot_label(Some(&file)),
                    viewport,
                });
            }
            Err(error) => {
                log(&format!("stress test: couldn't load {file}: {error}"));
                self.failed += 1;
            }
        }
        if self.queue.is_empty() {
            let elapsed = self.started.elapsed();
            self.loaded_in = Some(elapsed);
            log(&format!(
                "stress test: {} costumes loaded in {:.0} ms",
                self.cells.len(),
                elapsed.as_secs_f64() * 1000.0
            ));
        }
        cx.notify();
        !self.queue.is_empty()
    }

    /// The window's frame rate over its recent frames.
    fn fps(&self) -> f64 {
        match (self.frames.front(), self.frames.back()) {
            (Some(first), Some(last)) if self.frames.len() > 1 => {
                (self.frames.len() - 1) as f64 / last.duration_since(*first).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

/// The largest cell that fits `count` cells in `width` × `height`, and how
/// many go in a row.
fn cell_size(count: usize, width: f32, height: f32) -> (f32, usize) {
    (1..=count.max(1))
        .map(|columns| {
            let rows = count.max(1).div_ceil(columns);
            let cell = (width / columns as f32).min(height / (rows as f32 * ASPECT));
            (cell.floor(), columns)
        })
        .fold(
            (0.0, 1),
            |best, next| if next.0 > best.0 { next } else { best },
        )
}

impl Render for StressTest {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Every frame, to count the window's frames and keep the numbers live.
        window.request_animation_frame();
        let now = Instant::now();
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|first| now.duration_since(*first) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
        let palette = Theme::global(cx).palette;
        let drawing = self
            .cells
            .iter()
            .map(|cell| cell.viewport.read(cx).cpu_frame_ms())
            .fold(0.0, |total, ms| total + ms);
        let mut stats = vec![
            format!("{} of {} costumes", self.cells.len(), self.total),
            format!("{:.0} fps", self.fps()),
            format!("{drawing:.1} ms drawing per frame"),
        ];
        if let Some(loaded_in) = self.loaded_in {
            stats.push(format!(
                "loaded in {:.0} ms",
                loaded_in.as_secs_f64() * 1000.0
            ));
        }
        if self.failed > 0 {
            stats.push(format!("{} failed", self.failed));
        }
        // Mirror the numbers to stderr, so runs can be measured.
        if self
            .last_report
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(2))
        {
            self.last_report = Some(now);
            log(&format!("stress test: {}", stats.join(" · ")));
        }
        let (side, _) = self.area.map_or((0.0, 1), |area| {
            cell_size(
                self.total,
                f32::from(area.size.width),
                f32::from(area.size.height),
            )
        });
        let this = cx.entity();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(rem(space::MD))
                    .py(rem(space::XS))
                    .font_family(font::MONO)
                    .text_size(rem(text::XS))
                    .text_color(palette.muted.to_gpui())
                    .child(stats.join(" · ")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                this.update(cx, |test, _| test.area = Some(bounds));
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_wrap()
                            .content_start()
                            .children(self.cells.iter().enumerate().map(|(index, cell)| {
                                let label = cell.label.clone();
                                div()
                                    .id(("stress-cell", index))
                                    .relative()
                                    .w(px(side))
                                    .h(px(side * ASPECT))
                                    .tooltip(move |_, cx| Tooltip::new(label.clone()).view(cx))
                                    .child(cell.viewport.clone())
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{ASPECT, cell_size};

    #[test]
    fn every_cell_fits_the_area() {
        let (cell, columns) = cell_size(124, 1600.0, 900.0);
        assert!(columns as f32 * cell <= 1600.0);
        assert!(124_usize.div_ceil(columns) as f32 * cell * ASPECT <= 900.0);
        // 124 cells in 1600 × 900 fit at 90 px (16 columns, 8 rows).
        assert_eq!((cell, columns), (90.0, 16));
    }
}
