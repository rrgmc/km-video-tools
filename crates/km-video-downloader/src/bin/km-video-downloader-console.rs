//! The same program, from a shell.
//!
//! Console-subsystem on Windows — no `windows_subsystem` attribute — so `--help`, the address it is
//! serving at, and anything that goes wrong have somewhere to be read. It never opens a window of
//! its own; a browser is the way to the page from here.

fn main() -> anyhow::Result<()> {
    km_video_downloader::run(km_video_downloader::Shell::Console)
}
