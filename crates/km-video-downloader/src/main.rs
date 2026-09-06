//! The one somebody double-clicks. Everything it does is in the library beside this file.
//!
//! **`windows_subsystem = "windows"` is what stops a console appearing beside the application**, and
//! it is a linker setting rather than anything the program can decide — which is why the console
//! twin is a second binary rather than a flag. Only where there is a window to show instead: a build
//! without the `desktop` feature would otherwise have no window *and* nowhere to print, which is a
//! program that starts and vanishes.
#![cfg_attr(all(windows, feature = "desktop"), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    km_video_downloader::run(km_video_downloader::Shell::Windowed)
}
