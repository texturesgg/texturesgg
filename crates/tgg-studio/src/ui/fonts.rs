//! The design system's typefaces, bundled so the app looks the same on every
//! platform, whatever fonts the system has.
//!
//! Schibsted Grotesk and DM Mono come from google/fonts under the SIL Open Font
//! License 1.1 (no Reserved Font Name); each directory under `assets/fonts`
//! carries its `OFL.txt`. Schibsted Grotesk ships as static instances of the
//! upstream variable font at the weights the web uses, as tgg-ui's do.

/// Every face, for `AppSpec::fonts`.
pub static FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/schibsted-grotesk/SchibstedGrotesk-400.ttf"),
    include_bytes!("../../assets/fonts/schibsted-grotesk/SchibstedGrotesk-500.ttf"),
    include_bytes!("../../assets/fonts/schibsted-grotesk/SchibstedGrotesk-600.ttf"),
    include_bytes!("../../assets/fonts/schibsted-grotesk/SchibstedGrotesk-700.ttf"),
    include_bytes!("../../assets/fonts/schibsted-grotesk/SchibstedGrotesk-800.ttf"),
    include_bytes!("../../assets/fonts/dm-mono/DMMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/dm-mono/DMMono-Medium.ttf"),
];

/// The UI and mono families, for `AppSpec::families`.
pub const FAMILIES: (&str, &str) = ("Schibsted Grotesk", "DM Mono");
