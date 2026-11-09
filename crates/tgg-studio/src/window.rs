//! Where the window opens, and how big.

use rune::Display;

/// The window's size, on a screen with room for it.
pub const SIZE: (f32, f32) = (1280.0, 800.0);
/// The smallest the window goes.
pub const MIN_SIZE: (f32, f32) = (900.0, 600.0);

/// What the screen keeps beside the window's content: across, a little
/// space; down, the title bar, the taskbar (Windows) or menu bar and Dock
/// (macOS), and a little space.
const AROUND: (f32, f32) = (40.0, 120.0);

/// The window's size and, on Windows, its position on the primary display
/// in `displays`: [`SIZE`], shrunk so none of it is off a smaller screen
/// (a 1366×768 laptop), and on Windows centred on it, where it would
/// otherwise open part way down and to the right.
pub fn opening(displays: &[Display]) -> ((f32, f32), Option<(i32, i32)>) {
    // Headless runs (snapshots) keep the size they always had.
    let Some(display) = displays
        .iter()
        .find(|display| display.primary)
        .or(displays.first())
        .filter(|display| display.id.is_some())
    else {
        return (SIZE, None);
    };
    let scale = display.scale as f32;
    let screen = (display.size.0 as f32 / scale, display.size.1 as f32 / scale);
    let size = fit(screen);
    let position = cfg!(target_os = "windows").then(|| {
        // The title bar sits above the content; the taskbar takes the
        // bottom's share of what's around it.
        let left = ((screen.0 - size.0) / 2.0).max(0.0);
        let top = ((screen.1 - size.1 - AROUND.1) / 2.0).max(0.0);
        (
            display.position.0 + (left * scale).round() as i32,
            display.position.1 + (top * scale).round() as i32,
        )
    });
    (size, position)
}

/// [`SIZE`] on a screen of `screen` points, or less if it doesn't fit.
fn fit(screen: (f32, f32)) -> (f32, f32) {
    (
        SIZE.0.min(screen.0 - AROUND.0).max(MIN_SIZE.0),
        SIZE.1.min(screen.1 - AROUND.1).max(MIN_SIZE.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_screen_gets_a_smaller_window_and_a_big_one_the_usual() {
        assert_eq!(fit((1366.0, 768.0)), (1280.0, 648.0));
        assert_eq!(fit((1920.0, 1080.0)), SIZE);
        // Never below the window's minimum.
        assert_eq!(fit((800.0, 600.0)), MIN_SIZE);
        assert_eq!(opening(&[]).0, SIZE);
    }
}
