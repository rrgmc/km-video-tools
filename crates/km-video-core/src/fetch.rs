//! The whole sequence — preflight, download, check, re-encode — reported as events.
//!
//! # Why this is a library function and not a `main`
//!
//! It used to be a `main`. Every step below lived in `km-video-fetch`'s own `run()`, interleaved
//! with the `println!`s that reported it, which is a perfectly good shape right up to the moment a
//! second front end wants the same sequence. A web page cannot reuse a function that prints.
//!
//! So the sequence moved here and the printing stayed there. [`fetch`] does the work and calls
//! `on_event` as it goes; the command line renders those events as lines, and the web UI renders the
//! same events as a page. Neither knows anything the other does not.
//!
//! **That is the repository's own rule, now enforced rather than asserted.** `docs/decisions.md`
//! says a binary crate here is a command line and its output; before this module it was true only
//! because nothing had tested it.
//!
//! # The events are a narration, not a state machine
//!
//! They arrive in the order things happen and each one is complete in itself. A caller that ignores
//! every event still gets the [`Outcome`], and a caller that wants a running commentary gets one
//! without this module knowing what a commentary looks like.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::{args, check, profile, run};

/// How yt-dlp's own output is handled.
///
/// The one thing the two front ends genuinely differ on, which is why it is a parameter rather than
/// a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Inherit the terminal and let yt-dlp draw its own bar.
    ///
    /// Better than anything reconstructed from a pipe, and there is no pipe to deadlock on. The
    /// price is that nothing here can say how far along a download is — which costs a command line
    /// nothing, because the person running it is watching yt-dlp say so.
    Terminal,
    /// Pipe the output and report it, asking yt-dlp for parseable progress as well.
    ///
    /// What a caller with no terminal needs. [`Event::Downloading`] arrives about twice a second
    /// and [`Event::Said`] carries everything else yt-dlp wrote.
    Watched,
}

/// Everything a fetch depends on.
#[derive(Debug, Clone)]
pub struct Request {
    /// What to fetch and how, exactly as [`args::argv`] will read it.
    ///
    /// [`Plan::progress_lines`](args::Plan::progress_lines) is set from [`Request::progress`] and
    /// need not be filled in by the caller.
    pub plan: args::Plan,
    /// The yt-dlp to run, when it is not the one on `PATH`.
    pub yt_dlp: Option<PathBuf>,
    /// Re-encode anything that landed outside the profile.
    pub normalize: bool,
    /// What to do with yt-dlp's output.
    pub progress: Progress,
}

/// One thing that happened, as it happened.
#[derive(Debug, Clone)]
pub enum Event {
    /// Which yt-dlp is being used, and how old it is where that could be worked out.
    Tool {
        /// What `yt-dlp --version` said.
        version: String,
        /// Its age in days, where the version looks like a date. `None` for a nightly or a
        /// distribution's own numbering, which is not a fault.
        stale_days: Option<i64>,
    },
    /// The destination folder carried its own list, and this is it.
    ReadingList(PathBuf),
    /// The command about to run, quoted well enough to paste into a shell.
    Command(String),
    /// One line yt-dlp wrote. Only under [`Progress::Watched`].
    Said(String),
    /// How far one file has got. Only under [`Progress::Watched`].
    Downloading {
        /// The video's title, as the extractor reported it.
        title: String,
        /// 0 to 100.
        percent: f32,
        /// yt-dlp's own wording, or `None` while it does not know yet.
        speed: Option<String>,
        /// The same, for the estimate.
        eta: Option<String>,
    },
    /// yt-dlp has stopped and its records have been read.
    ///
    /// **Carries the count**, which is what lets a caller write a heading — "fetched 3 videos" —
    /// before the [`Event::Arrived`] events rather than having to hold them all and count them
    /// afterwards. A progress bar over the checking phase needs the same number.
    Downloaded {
        /// Whether yt-dlp exited successfully. False over a playlist usually means one video was
        /// private or region-locked and the rest still arrived.
        ok: bool,
        /// How many files it produced. Zero is ordinary: every url may already be in the archive.
        fetched: usize,
    },
    /// One file landed, and this is what it turned out to be.
    Arrived {
        /// What yt-dlp knew about it.
        record: run::Record,
        /// What the profile made of it.
        verdict: Verdict,
    },
    /// A re-encode is running. Emitted repeatedly for one file.
    Normalizing {
        /// The file being re-encoded.
        path: PathBuf,
        /// 0 to 100.
        percent: u8,
    },
}

/// What measuring one arrived file against the profile came to.
#[derive(Debug, Clone)]
pub enum Verdict {
    /// Nothing to do: packaging will copy its bytes.
    InProfile,
    /// It plays, and packaging will re-encode it.
    Outside {
        /// The mismatches, joined for reading.
        summary: String,
    },
    /// The machine cannot play it as it is. Only the pixel format can cause this.
    Unplayable {
        /// The mismatches, joined for reading.
        summary: String,
    },
    /// It was outside the profile and has been re-encoded into it.
    Normalized {
        /// What was wrong before the re-encode.
        summary: String,
    },
    /// The file is on disk and could not be read.
    ///
    /// **A finding rather than a failure**, so a run goes on to the next file. One unreadable
    /// download must not cost somebody the other nineteen.
    Unreadable {
        /// What went wrong, worded for a person.
        why: String,
    },
    /// A dry run: nothing was downloaded, so there is nothing to measure.
    NotFetched,
}

impl Verdict {
    /// Whether this file cannot be played as it stands.
    #[must_use]
    pub fn blocking(&self) -> bool {
        matches!(self, Self::Unplayable { .. })
    }
}

/// How a whole fetch came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// How many files the run produced.
    pub fetched: usize,
    /// Whether yt-dlp itself finished cleanly.
    pub completed: bool,
    /// Whether every file that arrived can be played as it is.
    pub all_playable: bool,
}

/// Runs one fetch from end to end.
///
/// Fails only for the things that stop a run before it starts — no yt-dlp, no ffmpeg, nothing to
/// fetch, a destination that cannot be created. Everything after that is reported as an event and
/// summarised in the [`Outcome`], because by then files exist and a caller needs to hear about them
/// rather than about an error.
pub fn fetch(request: &Request, mut on_event: impl FnMut(Event)) -> Result<Outcome> {
    let mut plan = request.plan.clone();
    plan.progress_lines = matches!(request.progress, Progress::Watched);

    if !plan.targets.is_empty() && plan.from_file.is_some() {
        bail!("give URLs or a list of them, not both");
    }

    // **A folder is allowed to say what goes in it.** With nothing named, a `km-video-fetch.txt` in
    // the destination is taken as the list — the same argument the archive beside it already makes:
    // what to fetch *into this folder* is a fact about the folder, and one somebody maintains by
    // hand over months rather than retypes.
    //
    // Said out loud, because a run that fetched forty videos nobody named must be able to say where
    // the forty came from.
    if plan.targets.is_empty() && plan.from_file.is_none() {
        plan.from_file = args::Plan::folders_own_list(&plan.out);
        match &plan.from_file {
            Some(list) => on_event(Event::ReadingList(list.clone())),
            None => bail!(
                "nothing to fetch — give a URL, or a list of them, or put one in {}",
                plan.out.join(args::BATCH_NAME).display()
            ),
        }
    }

    let binary = run::binary(request.yt_dlp.as_deref());
    let version = run::version(&binary)?;
    on_event(Event::Tool {
        stale_days: run::age_in_days(&version, run::today()),
        version,
    });
    run::ensure_ffmpeg()?;

    // Made now rather than left to yt-dlp, because the archive and the record file both live in it
    // and both are opened before the first download finishes.
    std::fs::create_dir_all(&plan.out)
        .with_context(|| format!("creating {}", plan.out.display()))?;

    // yt-dlp is given the bare name and resolves it against `-P`; this is the same file, spelled so
    // that it can be read back. Removed first, because a stale one from an interrupted run would
    // otherwise be reported as this run's haul.
    let records = args::Plan::records_path(&plan.out);
    let _ = std::fs::remove_file(&records);

    let argv = args::argv(&plan);
    on_event(Event::Command(command_line(&binary, &argv)));

    let completed = match request.progress {
        Progress::Terminal => run::spawn(&binary, &argv)?,
        Progress::Watched => {
            run::spawn_watched(&binary, &argv, |line| match progress_line(line) {
                Some(event) => on_event(event),
                None => on_event(Event::Said(line.to_owned())),
            })?
        }
    };

    let fetched = run::read_records(&records)?;
    let _ = std::fs::remove_file(&records);
    on_event(Event::Downloaded {
        ok: completed,
        fetched: fetched.len(),
    });

    // Looked up once for the whole run rather than per file: it shells out to `ffmpeg -encoders`,
    // and asking twenty times what the answer was the first time is twenty subprocesses.
    //
    // **Only where a re-encode might actually happen.** An ffmpeg with no H.264 encoder is an
    // unusual build, and refusing to *report* on a download because of it would be absurd.
    let encoders = if request.normalize && !plan.dry_run {
        Some(profile::encoders()?)
    } else {
        None
    };

    let mut all_playable = true;
    for record in fetched.iter().cloned() {
        let verdict = if plan.dry_run {
            Verdict::NotFetched
        } else {
            match record.path() {
                Some(path) => inspect(&path, encoders.as_ref(), &mut on_event),
                None => Verdict::Unreadable {
                    why: "no file was written".to_owned(),
                },
            }
        };
        all_playable &= !verdict.blocking();
        on_event(Event::Arrived { record, verdict });
    }

    Ok(Outcome {
        fetched: fetched.len(),
        completed,
        all_playable,
    })
}

/// Measures one arrived file, re-encoding it when `encoders` says to.
fn inspect(
    path: &Path,
    encoders: Option<&profile::Encoders>,
    on_event: &mut impl FnMut(Event),
) -> Verdict {
    let report = match check::inspect(path) {
        Ok(report) => report,
        // Not fatal, and not a lie either: the file is on disk and this could not read it.
        Err(error) => {
            return Verdict::Unreadable {
                why: format!("{error:#}"),
            };
        }
    };

    if report.in_profile() {
        return Verdict::InProfile;
    }

    let summary = report
        .mismatches
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");

    let Some(encoders) = encoders else {
        return if report.blocking() {
            Verdict::Unplayable { summary }
        } else {
            Verdict::Outside { summary }
        };
    };

    let owned = path.to_path_buf();
    match check::normalize(&report, encoders, |progress| {
        on_event(Event::Normalizing {
            path: owned.clone(),
            percent: progress.percent(),
        });
    }) {
        Ok(_) => Verdict::Normalized { summary },
        Err(error) => Verdict::Unreadable {
            why: format!("re-encoding failed: {error:#}"),
        },
    }
}

/// Reads one [`args::PROGRESS_TEMPLATE`] line, or `None` for anything else yt-dlp wrote.
///
/// Everything here is defensive on purpose. This parses another program's output, that output is
/// space-padded to a fixed width, and two of its four fields have literal `Unknown` and `NA` values
/// in ordinary use — so a field that will not parse becomes `None` and never a failure. The
/// alternative is a download that dies because a title had an `=` in it.
fn progress_line(line: &str) -> Option<Event> {
    let rest = line.strip_prefix("KMP ")?;

    // `title` is last in the template and may itself contain spaces and `=`, so the fields are cut
    // from the front by name rather than split apart.
    let field = |name: &str| -> Option<&str> {
        let at = rest.find(&format!("{name}="))? + name.len() + 1;
        // **The leading trim is the whole trap.** yt-dlp pads every value to a fixed width, so
        // `pct=  2.4%` splits on its *first* space into an empty string — which parses as nothing
        // and puts a bar that never moves on the page. Found by running it.
        let value = rest[at..].trim_start();
        Some(match name {
            // Last in the template precisely so it may contain spaces, and video titles do.
            "title" => value,
            _ => value.split_once(' ').map_or(value, |(head, _)| head),
        })
    };

    // A `finished` line is the end of one file, not of the run, and it always reads 100%. Reported
    // as such so a bar lands on full rather than stopping at 97.
    let percent = field("pct")
        .map(|value| value.trim().trim_end_matches('%'))
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);

    let unknown = |value: &str| {
        let value = value.trim();
        (!value.is_empty() && value != "NA" && !value.starts_with("Unknown"))
            .then(|| value.to_owned())
    };

    Some(Event::Downloading {
        title: field("title").unwrap_or_default().trim().to_owned(),
        percent: percent.clamp(0.0, 100.0),
        speed: field("speed").and_then(unknown),
        eta: field("eta").and_then(unknown),
    })
}

/// The command line, quoted enough to be pasted back into a shell.
#[must_use]
pub fn command_line(binary: &std::ffi::OsStr, argv: &[std::ffi::OsString]) -> String {
    let mut line = quote(&binary.to_string_lossy());
    for arg in argv {
        line.push(' ');
        line.push_str(&quote(&arg.to_string_lossy()));
    }
    line
}

/// Quotes one argument if it needs it.
fn quote(value: &str) -> String {
    if value.is_empty() || value.contains([' ', '"', '\'', '*', '?', '(', ')', '&', '|', '<', '>'])
    {
        format!("'{}'", value.replace('\'', r"'\''"))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from yt-dlp 2026.07.04 rather than written by hand. The padding is real, and so is
    /// `Unknown B/s` on an early line.
    const EARLY: &str =
        "KMP status=downloading pct=  2.4% speed= Unknown B/s eta=Unknown title=in-profile";
    const MOVING: &str =
        "KMP status=downloading pct= 49.4% speed=  20.49MiB/s eta=00:12 title=in-profile";
    const LAST: &str = "KMP status=finished pct=100.0% speed=16.57MiB/s eta=NA title=in-profile";

    #[test]
    fn a_progress_line_is_read_padding_and_all() {
        let Some(Event::Downloading {
            title,
            percent,
            speed,
            eta,
        }) = progress_line(MOVING)
        else {
            panic!("wanted a progress event");
        };
        assert!((percent - 49.4).abs() < 0.01, "{percent}");
        assert_eq!(speed.as_deref(), Some("20.49MiB/s"));
        assert_eq!(eta.as_deref(), Some("00:12"));
        assert_eq!(title, "in-profile");
    }

    /// Both of these are ordinary values rather than faults: speed is unknown for the first second
    /// of every download, and eta is `NA` on the line that says a file is done.
    #[test]
    fn unknown_and_na_become_nothing_rather_than_words() {
        let Some(Event::Downloading { speed, eta, .. }) = progress_line(EARLY) else {
            panic!("wanted a progress event");
        };
        assert_eq!(speed, None, "`Unknown B/s` is not a speed");
        assert_eq!(eta, None);

        let Some(Event::Downloading { eta, percent, .. }) = progress_line(LAST) else {
            panic!("wanted a progress event");
        };
        assert_eq!(eta, None, "`NA` is not an estimate");
        assert!(
            (percent - 100.0).abs() < f32::EPSILON,
            "a finished file is full"
        );
    }

    /// Everything else yt-dlp writes goes through untouched, which is the other half of the
    /// contract: those lines are the diagnosis when a download fails.
    #[test]
    fn yt_dlps_own_words_are_not_mistaken_for_progress() {
        assert!(progress_line("[youtube] abc: Downloading webpage").is_none());
        assert!(progress_line("[download] Destination: A Song.mp4").is_none());
        assert!(progress_line("ERROR: unable to download video data").is_none());
        assert!(progress_line("").is_none());
        assert!(
            progress_line("KMP").is_none(),
            "the prefix alone is not a line"
        );
    }

    /// A title is the last field precisely so it may contain anything, and video titles do.
    #[test]
    fn a_title_may_contain_spaces_and_an_equals_sign() {
        let Some(Event::Downloading { title, percent, .. }) = progress_line(
            "KMP status=downloading pct= 10.0% speed=1MiB/s eta=00:01 title=E=mc2 (Live) - A Band",
        ) else {
            panic!("wanted a progress event");
        };
        assert_eq!(title, "E=mc2 (Live) - A Band");
        assert!((percent - 10.0).abs() < f32::EPSILON);
    }

    /// The point is that what is printed can be pasted straight back into a shell — a format
    /// selector full of `*` and `[` is otherwise the shell's to interpret, not yt-dlp's.
    #[test]
    fn a_pasteable_command_line_quotes_what_needs_it() {
        let line = command_line(
            std::ffi::OsStr::new("yt-dlp"),
            &[
                std::ffi::OsString::from("-f"),
                std::ffi::OsString::from("bv*[height<=1080]+ba/b"),
                std::ffi::OsString::from("-P"),
                std::ffi::OsString::from("my videos"),
            ],
        );
        assert_eq!(line, "yt-dlp -f 'bv*[height<=1080]+ba/b' -P 'my videos'");
    }
}
