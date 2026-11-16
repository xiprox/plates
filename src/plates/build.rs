//! Gives plates.exe its icon, the one Explorer and shortcuts to Plates show, as a resource in the
//! program. GPUI's own resource in plates.exe is a manifest, so the two don't collide. (read in
//! gpui-pre 0.3.12, resources/windows/gpui.rc)

fn main() {
    println!("cargo:rerun-if-changed=icons/plates.ico");
    // The resource compiler is the Windows SDK's rc.exe, so a build for Windows from another OS,
    // such as `cargo check --target x86_64-pc-windows-msvc` on a Mac, goes without the icon.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || !cfg!(windows) {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("icons/plates.ico")
        .set("FileDescription", "Plates")
        .set("ProductName", "Plates");
    resource
        .compile()
        .expect("plates.exe's icon should compile with the Windows SDK's rc.exe");
}
