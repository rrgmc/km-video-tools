//! Starting a child process without giving it a console window of its own.
//!
//! # Why this exists
//!
//! **Not cosmetic, and it is the window this repository is laid out for that needs it.** A
//! GUI-subsystem executable on Windows has no console for a child to inherit, so Windows hands each
//! child one of its own — a black window per file, through a batch that may be hundreds of them.
//! `km-video-downloader` is exactly such an executable; see `docs/decisions.md`.
//!
//! # The rule, which is what decides each call site
//!
//! **Hide the console wherever the child's output is captured or discarded; never where the child
//! is handed a terminal on purpose.**
//!
//! The second half is not a hedge. `CREATE_NO_WINDOW` detaches a child from the parent's console
//! rather than merely suppressing a new one, so it is not free at a call site that inherits — and
//! it buys nothing there either, because a console-subsystem parent already has a console and
//! Windows creates no second one. [`crate::run::spawn`] is the one such call in this crate, and it
//! deliberately goes without: yt-dlp's own progress bar drawn into the terminal it was given is
//! better than anything this could rebuild from a pipe.
//!
//! Everywhere else — [`crate::run::version`], [`crate::run::ensure_ffmpeg`],
//! [`crate::run::spawn_watched`], [`crate::probe::probe`], [`crate::profile::encoders`],
//! [`crate::profile::transcode`] — already captures or discards what the child says, so the window
//! was never showing anybody anything.

use std::process::Command;

/// The flag itself, from `winbase.h`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Starts this command without a console window, and is a no-op off Windows.
///
/// Returns the command so it can be wrapped around a `Command::new` in an expression, which is how
/// most call sites here read.
pub fn without_a_console_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
