//! Opening the page in whatever browser this computer uses.
//!
//! Fifteen lines rather than a dependency. The karaoke app has a crate for this — it has to,
//! because it opens files of half a dozen kinds from three programs, on a phone as well as a
//! desktop. Here there is one caller and one kind of thing to open, and the whole of the platform
//! difference is which of three commands to run.
//!
//! **Failing to open a browser is not a failure to start.** The banner has already said where the
//! page is; if this cannot manage it, the answer is one sentence and a URL somebody can paste.

use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use km_video_core::child::without_a_console_window;

/// Asks the desktop to open one URL.
///
/// Only ever called with this program's own `http://127.0.0.1:<port>/`, which is why nothing here
/// escapes or validates: there is no user input to protect against.
pub fn open(url: &str) -> Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        // **Through `cmd /c start`, and the empty `""` is load-bearing.** `start` reads its first
        // quoted argument as a window title, so `start "http://…"` opens an empty console window
        // titled with the URL and no browser at all.
        let mut command = Command::new("cmd");
        command.args(["/c", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    // Nothing is read back, and a browser that writes to stderr on startup — several do — must not
    // have that appear in the middle of this program's own output.
    //
    // **And no console window either**, which is `cmd /c start` above rather than the other two: a
    // GUI-subsystem executable has no console for a child to inherit, so Windows hands `cmd` one of
    // its own and the application opens behind a black window that flashes and goes. On the shared
    // command rather than in that branch, because it is a no-op everywhere else.
    without_a_console_window(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("could not open {url}"))?;
    Ok(())
}
