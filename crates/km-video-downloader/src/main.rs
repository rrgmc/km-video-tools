//! The one somebody double-clicks. Everything it does is in the library beside this file.
//!
//! **`windows_subsystem = "windows"` is what stops a console appearing beside the application**, and
//! it is a linker setting rather than anything the program can decide — which is why the console
//! twin is a second binary rather than a flag. Only where there is a window to show instead: a build
//! without the `desktop` feature would otherwise have no window *and* nowhere to print, which is a
//! program that starts and vanishes.
#![cfg_attr(all(windows, feature = "desktop"), windows_subsystem = "windows")]

//! **And the failure on the way out is reported the same way**: an `Err` from here is written to a
//! standard error nobody double-clicking this will ever read, so [`alert::show_fatal`] puts it on
//! the screen where there is no console to read it in. See that module for why the check is on the
//! terminal rather than on the build.
//!
//! [`alert::show_fatal`]: km_video_downloader::alert::show_fatal
fn main() -> anyhow::Result<()> {
    km_video_downloader::run(km_video_downloader::Shell::Windowed)
        .inspect_err(km_video_downloader::alert::show_fatal)
}
