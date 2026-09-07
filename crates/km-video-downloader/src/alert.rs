//! Saying a fatal startup failure out loud where there is no console to say it into.
//!
//! # The gap this fills
//!
//! `main` returns a `Result`, and an `Err` out of it is written to standard error. That is the right
//! arrangement everywhere except the one place this program is actually started from: a
//! double-click. A GUI-subsystem executable on Windows has a null standard error, and an application
//! bundle launched by LaunchServices has one nobody will ever read — so the program vanishes on
//! startup and leaves nothing behind, which is the same failure `say` exists for and the same one
//! `handoff` removed for a taken port.
//!
//! [`handoff`](crate::handoff) covers the common cause. What is left is everything else: a port held
//! by a program that is *not* this one, a `--data-dir` that cannot be made, an async runtime that
//! will not start. Rare, and each one currently indistinguishable from the program not existing.
//!
//! # Why a terminal check rather than a flag
//!
//! **The question is not which build this is, it is whether anybody can read what was already
//! printed.** `Shell::Console`, the `desktop` feature and `windows_subsystem` are all proxies for
//! that and each is wrong somewhere — the windowed build run from a terminal has a console, and a
//! console build launched from a file manager has none. `stderr().is_terminal()` asks the real
//! question, so a dialog appears exactly where the message would otherwise have gone nowhere.
//!
//! # Why only macOS
//!
//! `osascript` is in the base system and this program already shells out to `open` for the browser
//! — one more child process, no dependency, in the shape [`crate::opener`] is already in.
//!
//! Windows would want `MessageBoxW` and therefore `windows-sys`, which would be the first such crate
//! in a tree whose whole build story is a handful of crates and no C compiler. It also already has
//! the answer this would duplicate: `km-video-downloader-console.exe` is a second binary that exists
//! precisely to be the build that can talk, and it is what somebody is told to run when the windowed
//! one will not start.

/// Shows a fatal error where standard error cannot be read, and does nothing where it can.
///
/// Called on the way out of `main` in the windowed binary. Every failure inside is dropped: this is
/// the reporter of last resort, and there is nowhere to report a failure to report.
pub fn show_fatal(error: &anyhow::Error) {
    #[cfg(target_os = "macos")]
    {
        use std::io::IsTerminal as _;

        // Already readable, so a dialog would be a second copy of a message somebody is looking at.
        if std::io::stderr().is_terminal() {
            return;
        }
        show(&described(error));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = error;
    }
}

/// The whole chain, one cause per line, capped.
///
/// **The chain and not just the outermost sentence**, because the outer ones here are written for a
/// person and the inner one is what actually happened — `binding 127.0.0.1:8181` above
/// `Address already in use` is the pair that identifies the fault. Capped because an error is not a
/// log and a dialog is not a window somebody can scroll.
#[cfg(target_os = "macos")]
fn described(error: &anyhow::Error) -> String {
    const LIMIT: usize = 600;

    let mut said = String::new();
    for cause in error.chain() {
        if !said.is_empty() {
            said.push_str("\n\n");
        }
        said.push_str(&cause.to_string());
        if said.len() >= LIMIT {
            break;
        }
    }
    if said.chars().count() > LIMIT {
        said = said.chars().take(LIMIT).collect::<String>() + "…";
    }
    said
}

/// Puts one sentence on the screen through `osascript`, and waits for it to be dismissed.
///
/// **Waited on rather than spawned.** The caller is on its way out of `main`, and a dialog belonging
/// to a process that has already exited goes with it.
#[cfg(target_os = "macos")]
fn show(said: &str) {
    use std::process::{Command, Stdio};

    let script = format!(
        r#"display dialog "{}" with title "{}" buttons {{"OK"}} default button "OK" with icon stop"#,
        applescript(said),
        applescript(crate::server::APP_NAME),
    );

    let _ = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Escapes a string for an AppleScript literal.
///
/// Backslash first and then the quote, which is the order that matters: escaping the quote first
/// would put a backslash in front of it that the next pass would then escape again.
#[cfg(target_os = "macos")]
fn applescript(value: &str) -> String {
    value.replace('\\', r"\\").replace('"', "\\\"")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// A path with a quote in it must not end the literal it is inside.
    ///
    /// The failure this pins is not a mangled dialog: an unbalanced quote makes the whole script
    /// unparseable, so the report of last resort is the thing that fails to appear.
    #[test]
    fn a_quote_in_the_message_cannot_end_the_script() {
        let said = applescript(r#"no folder at C:\a\"b" — really"#);
        assert!(!said.contains(r#"""#) || said.contains(r#"\""#));
        assert_eq!(said, r#"no folder at C:\\a\\\"b\" — really"#);
    }

    /// The chain is joined, and a long one is cut rather than shown whole.
    #[test]
    fn every_cause_is_said_and_a_long_one_is_cut() {
        let error = anyhow::anyhow!("Address already in use").context("binding 127.0.0.1:8181");
        let said = described(&error);
        assert!(said.contains("binding 127.0.0.1:8181"), "{said}");
        assert!(said.contains("Address already in use"), "{said}");

        let long = anyhow::anyhow!("x".repeat(5_000));
        assert!(described(&long).chars().count() <= 601);
    }
}
