//! Everything `km-video-fetch` does that is not a command line.
//!
//! # Why this is a library and not one binary crate
//!
//! Because the repository is meant to hold more than one program. The command line is one way to
//! ask for a video; a local web UI, in the shape karaokemachine's `km-package-builder` and
//! `km-admin` already settled on, is the other one planned. Both want the same four things, and
//! none of the four has anything to say about how it was asked for:
//!
//! * [`args`] — which arguments yt-dlp gets, and why each one.
//! * [`run`] — finding yt-dlp, running it, and reading back what it did.
//! * [`probe`] — what a file that landed says about itself, read through `ffprobe`.
//! * [`profile`] — the shape a karaoke package wants, and the re-encode that reaches it.
//! * [`check`] — the three of those in the order that makes a verdict.
//!
//! So the rule for this repository is the one that fell out of the split: **a binary crate here is a
//! command line and its output, and nothing else.** Every `println!` in `km-video-fetch` is in its
//! `main.rs`; nothing in this crate prints at all.
//!
//! # What this needs installed
//!
//! Two other programs, neither bundled and neither linked:
//!
//! * **yt-dlp**, which does the downloading. Overridable with `--yt-dlp` because it is very often
//!   installed by `pipx` or into a virtualenv and genuinely often not on `PATH`.
//! * **ffmpeg**, which yt-dlp muxes with, [`profile`] re-encodes with, and [`probe`] reads with.
//!   Not overridable: a machine that has yt-dlp working at all has this.
//!
//! Nothing here links either of them, which is what keeps this a plain Rust build with four
//! dependencies from crates.io and no C toolchain, no bindgen and no libclang.

pub mod args;
pub mod check;
pub mod probe;
pub mod profile;
pub mod run;
