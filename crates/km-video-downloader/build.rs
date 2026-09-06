//! Puts the application icon and version information inside the Windows executable.
//!
//! **This is the only way Explorer and the taskbar get an icon before the process starts.** A window
//! can set its own icon once it exists, and that is too late for the file in a folder, the entry in
//! the Start menu, and the taskbar button in the moment between a double-click and the first frame.
//! Those all read the executable's own resource, which is what `winresource` attaches here.
//!
//! **A missing resource compiler is a warning rather than a failure.** `rc.exe` comes with the
//! Windows SDK, and somebody building this from a plain rustup install may not have one — which is
//! a reason for a plainer icon and not a reason to be unable to build the program at all.
//!
//! `ProductName` is the same string for both binaries on purpose: they are one product, which is
//! the distinction Windows keeps `ProductName` and `FileDescription` apart for. `FileDescription`
//! is what separates them and comes from this crate's own `description` — neither is picked up
//! automatically, as `winresource` defaults both to the crate name.

fn main() {
    // Host and target are two questions. The `#[cfg]` keeps this compiling where `winresource` is
    // not in the build; the check inside keeps a resource compiler away from a non-Windows target.
    #[cfg(windows)]
    attach_resources();
}

#[cfg(windows)]
fn attach_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let icon = "../../icon/km-video-downloader.ico";
    println!("cargo:rerun-if-changed={icon}");

    // Generated rather than committed-and-forgotten: `cargo run -p km-video-downloader --example
    // icon` writes it. A checkout that has not run it yet still builds, with Windows' default icon.
    if !std::path::Path::new(icon).exists() {
        println!(
            "cargo:warning=no application icon yet; run `cargo run -p km-video-downloader \
             --example icon` to draw one"
        );
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon);
    resource.set("ProductName", "KM Video Tools");
    if let Ok(description) = std::env::var("CARGO_PKG_DESCRIPTION") {
        resource.set("FileDescription", &description);
    }
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not attach the application icon: {error}");
        println!(
            "cargo:warning=km-video-downloader will build and run with Windows' default icon and \
             no version information. This needs rc.exe from the Windows SDK."
        );
    }
}
