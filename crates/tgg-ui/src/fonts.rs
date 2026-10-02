//! The design system's typefaces, bundled so the editor looks the same on
//! every platform, whatever fonts the system has.
//!
//! Schibsted Grotesk and DM Mono come from google/fonts under the SIL Open Font
//! License 1.1 (no Reserved Font Name); each directory under `assets/fonts`
//! carries its `OFL.txt`. gpui-ce's text system doesn't apply a variable
//! font's weight axis, so Schibsted Grotesk ships as static instances of the
//! upstream variable font at the weights the web uses, cut with
//! `fonttools varLib.instancer SchibstedGrotesk[wght].ttf wght=<weight>
//! --update-name-table`.

use gpui::App;
use std::borrow::Cow;

const FONTS: [&[u8]; 7] = [
    include_bytes!("../assets/fonts/schibsted-grotesk/SchibstedGrotesk-400.ttf"),
    include_bytes!("../assets/fonts/schibsted-grotesk/SchibstedGrotesk-500.ttf"),
    include_bytes!("../assets/fonts/schibsted-grotesk/SchibstedGrotesk-600.ttf"),
    include_bytes!("../assets/fonts/schibsted-grotesk/SchibstedGrotesk-700.ttf"),
    include_bytes!("../assets/fonts/schibsted-grotesk/SchibstedGrotesk-800.ttf"),
    include_bytes!("../assets/fonts/dm-mono/DMMono-Regular.ttf"),
    include_bytes!("../assets/fonts/dm-mono/DMMono-Medium.ttf"),
];

/// Register the bundled fonts with the app's text system.
pub fn load(cx: &mut App) -> anyhow::Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
}
