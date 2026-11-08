//! On Windows, the app's icon and version details go into tgg-studio.exe,
//! so Explorer, the taskbar and the installer show them. Nothing to do on
//! other systems.

fn main() {
    println!("cargo:rerun-if-changed=../../dist/windows/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("../../dist/windows/icon.ico")
        .set("ProductName", "textures.gg")
        .set("FileDescription", "textures.gg")
        .set("CompanyName", "SaavyLab LLC")
        .set("LegalCopyright", "© 2026 SaavyLab LLC");
    resource
        .compile()
        .expect("the Windows resources (icon, version) compile");
}
