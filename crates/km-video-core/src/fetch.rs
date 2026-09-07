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

use crate::{args, check, list, profile, run};

/// Whether to keep going, said by the same closure that hears about everything else.
///
/// **A return value rather than a flag the caller also holds**, because there is exactly one thing
/// already being called at every point where stopping is possible, and giving it a second job costs
/// nothing. A caller that never wants to stop returns [`Flow::Go`] and forgets about it.
pub use crate::run::Flow;

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
    ///
    /// **One per fetch, however many yt-dlp runs it took.** A marked list is several runs (see
    /// [`crate::list`]) and this is emitted once when the last of them has finished, carrying their
    /// combined haul. A second one would overwrite a caller's total with the last run's count alone
    /// — which, where that run fetched nothing, reads as *nothing is known yet* and leaves a
    /// progress bar sweeping for the rest of a run that succeeded.
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
    /// Whether the caller asked it to stop before it was done.
    pub stopped: bool,
}

/// Runs one fetch from end to end.
///
/// Fails only for the things that stop a run before it starts — no yt-dlp, no ffmpeg, nothing to
/// fetch, a destination that cannot be created. Everything after that is reported as an event and
/// summarised in the [`Outcome`], because by then files exist and a caller needs to hear about them
/// rather than about an error.
pub fn fetch(request: &Request, mut on_event: impl FnMut(Event) -> Flow) -> Result<Outcome> {
    let mut plan = request.plan.clone();
    plan.progress_lines = matches!(request.progress, Progress::Watched);

    if !plan.targets.is_empty() && plan.from_file.is_some() {
        bail!("give URLs or a list of them, not both");
    }

    // **Refused here rather than by yt-dlp.** An unknown browser is not a usage error yt-dlp turns
    // down at the door: it starts, extracts, and fails on the first video with a message that reads
    // like the site said no. A front end may well check this earlier and word it better — the web
    // UI does — but the library will not build a command it knows cannot work.
    if let Some(browser) = &plan.cookies_from_browser
        && !args::browser_is_known(browser)
    {
        bail!(
            "`{browser}` is not a browser yt-dlp can read cookies from. It knows {}.",
            args::COOKIE_BROWSERS.join(", ")
        );
    }

    // **A folder is allowed to say what goes in it.** With nothing named, a `km-video-fetch.kmvf` in
    // the destination is taken as the list — the same argument the archive beside it already makes:
    // what to fetch *into this folder* is a fact about the folder, and one somebody maintains by
    // hand over months rather than retypes.
    //
    // Said out loud, because a run that fetched forty videos nobody named must be able to say where
    // the forty came from.
    if plan.targets.is_empty() && plan.from_file.is_none() {
        plan.from_file = args::Plan::folders_own_list(&plan.out);
        match &plan.from_file {
            Some(list) => {
                on_event(Event::ReadingList(list.clone()));
            }
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

    // One run, or the several a marked list asks for. Refuses here — before anything is downloaded
    // — for the one thing a list can say that this will not do, which is name a folder outside the
    // one the fetch was pointed at.
    //
    // **The `Scratch` comes back with it**, holding whatever was written — including the list handed
    // in, where a front end wrote that. Gone however this function returns; there is a `?` between
    // the runs, and `runs` itself has one between the files it writes.
    let (plans, _scratch) = runs(&plan)?;

    // Carried with the destination that produced each one, for the deduplication below.
    let mut fetched: Vec<(PathBuf, run::Record)> = Vec::new();
    let mut completed = true;
    let mut stopped = false;

    for sub in &plans {
        // **Between the runs, which is where a stop has to take effect.** Somebody who pressed Stop
        // during a list of single videos did not mean *and now start the playlist*.
        if stopped {
            break;
        }

        // Per run, and each run's own: a destination is `-P`, and `-P` is what resolves both of
        // these names.
        std::fs::create_dir_all(&sub.out)
            .with_context(|| format!("creating {}", sub.out.display()))?;

        // yt-dlp is given the bare name and resolves it against `-P`; this is the same file,
        // spelled so that it can be read back. Removed first, because `--print-to-file` *appends* —
        // so a stale file from an interrupted run, or the one the previous run left, would
        // otherwise be counted as this run's haul.
        let records = args::Plan::records_path(&sub.out);
        let _ = std::fs::remove_file(&records);

        let argv = args::argv(sub);
        if on_event(Event::Command(command_line(&binary, &argv))) == Flow::Stop {
            stopped = true;
            break;
        }

        let (ok, asked_to_stop) = match request.progress {
            // Nothing to stop against: the child owns the terminal, so Ctrl-C reaches it directly
            // and is a better answer than anything this could arrange. It reaches this process too,
            // so there is no next run to worry about either.
            Progress::Terminal => (run::spawn(&binary, &argv)?, false),
            Progress::Watched => {
                // **Captured here rather than asked for afterwards.** `run::spawn_watched`
                // deliberately folds a stop into success — a killed child exits unsuccessfully and
                // reporting that as a failure would tell somebody who pressed Stop that yt-dlp had
                // broken — so this closure is the only place the answer exists. `|=` rather than
                // `=`, because collected stderr is replayed through the sink *after* the kill and a
                // plain assignment would lose it.
                let mut asked = false;
                let ok = run::spawn_watched(&binary, &argv, |line| {
                    let flow = match progress_line(line) {
                        Some(event) => on_event(event),
                        None => on_event(Event::Said(line.to_owned())),
                    };
                    asked |= flow == Flow::Stop;
                    flow
                })?;
                (ok, asked)
            }
        };
        completed &= ok;
        stopped |= asked_to_stop;

        // **Kept only where this destination has not already reported it**, which is a thing only a
        // dry run can produce: in an ordinary run the folder's own archive stops a video named both
        // on its own line and inside a marked playlist arriving twice, but `--simulate` writes no
        // archive, so both of that folder's runs report it and the dry run would promise one more
        // video than the real one delivers.
        //
        // **Per destination rather than across the whole fetch**, because the same video asked for
        // in two folders is two files and was asked for twice on purpose.
        for record in run::read_records(&records)? {
            let seen = record.id.is_some()
                && fetched
                    .iter()
                    .any(|(out, already)| out == &sub.out && already.id == record.id);
            if !seen {
                fetched.push((sub.out.clone(), record));
            }
        }
        let _ = std::fs::remove_file(&records);
    }

    let fetched: Vec<run::Record> = fetched.into_iter().map(|(_, record)| record).collect();

    // One event for the whole fetch, however many runs it took. See [`Event::Downloaded`].
    stopped |= on_event(Event::Downloaded {
        ok: completed,
        fetched: fetched.len(),
    }) == Flow::Stop;

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
    let mut checked = 0;
    for record in fetched.iter().cloned() {
        // **Between files, which is the only place stopping is worth doing here.** A re-encode is
        // minutes of work per song, and somebody who has pressed Stop halfway through a batch of
        // twenty means the other nineteen.
        if stopped {
            break;
        }
        checked += 1;
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
        stopped |= on_event(Event::Arrived { record, verdict }) == Flow::Stop;
    }

    Ok(Outcome {
        // What was looked at, rather than what yt-dlp wrote. A run stopped halfway must not claim
        // to have checked the files it never reached.
        fetched: checked,
        completed,
        all_playable,
        stopped,
    })
}

/// The yt-dlp runs one plan asks for: the one it has always made, or the several a marked list
/// needs.
///
/// **The split is here rather than in [`args::argv`]**, which builds an argv and spawns nothing and
/// does no file I/O at all. Deciding how many runs there are means reading the list, and the module
/// that chooses arguments should stay a pure function of the struct it is given — that separation is
/// the reason every one of those arguments can be asserted by value. This is the same shape: a pure
/// function of a plan and a file, so which runs a list produces is assertable too.
///
/// **One run per distinct pair of `(expand, destination)`**, because `--yes-playlist` /
/// `--no-playlist` and `-P` are both properties of an invocation and yt-dlp offers no per-URL form
/// of either.
///
/// **Every single-video run first, then every playlist run.** Three reasons, and the third is why
/// the order is fixed rather than derived from the flag: a folder's archive makes the first run win
/// a duplicate, and *this one video* is the more specific statement than *this playlist that happens
/// to contain it*; the single-video runs are the fast half, so somebody watching sees their named
/// picks land before a two-hundred-item playlist starts; and an order that depended on a checkbox
/// would be worse to reason about and worse to test.
fn runs(plan: &args::Plan) -> Result<(Vec<args::Plan>, Scratch)> {
    // The list handed in is itself scratch where a front end wrote it, and is spent either way: for
    // a marked list it is read and replaced by the files below, and for an unmarked one it is passed
    // straight to yt-dlp. `writing` keeps only what this tool writes itself, so a hand-maintained
    // list arriving here is left where it is.
    let mut scratch = Scratch::default();
    scratch.writing(plan.from_file.as_deref());

    let Some(from_file) = &plan.from_file else {
        return Ok((vec![plan.clone()], scratch));
    };

    let entries = list::read(from_file);
    // **A list that says nothing is handed over unread and unrewritten**, byte for byte the argv
    // this tool has always built. That is what keeps a `km-video-fetch.kmvf` somebody maintains by
    // hand from being rewritten behind their back, and what keeps a list carrying things
    // [`crate::list`] does not model — a `;` comment, an option yt-dlp itself understands — working
    // exactly as it did. An unreadable list says nothing, and is yt-dlp's to complain about.
    if entries
        .iter()
        .all(|entry| entry.expand.is_none() && entry.out.is_none())
    {
        return Ok((vec![plan.clone()], scratch));
    }

    // Grouped in first-mention order, so the runs come out in the order the list reads.
    let mut groups: Vec<(bool, PathBuf, Vec<list::Entry>)> = Vec::new();
    for entry in entries {
        let expand = entry.expands(plan.playlist);
        let out = match &entry.out {
            Some(said) => list::destination(&plan.out, said)?,
            None => plan.out.clone(),
        };
        match groups
            .iter_mut()
            .find(|(kind, folder, _)| *kind == expand && folder == &out)
        {
            Some((_, _, lines)) => lines.push(entry),
            None => groups.push((expand, out, vec![entry])),
        }
    }

    let mut plans = Vec::with_capacity(groups.len());
    // `false` before `true`: the single-video runs first. See this function's header.
    for wanted in [false, true] {
        for (expand, out, lines) in groups.iter().filter(|(kind, _, _)| *kind == wanted) {
            let name = if *expand {
                args::PLAYLISTS_NAME
            } else {
                args::SINGLES_NAME
            };
            let path = out.join(name);
            std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
            // Registered before it is written, so a `write_urls` that fails halfway still leaves
            // behind something this knows to remove.
            scratch.writing(Some(&path));
            // Markers stripped: they have already been spent deciding which run this is, and yt-dlp
            // would take one at the front of a line as part of the URL it was given.
            list::write_urls(&path, lines)
                .with_context(|| format!("writing {}", path.display()))?;
            plans.push(args::Plan {
                targets: Vec::new(),
                from_file: Some(path),
                playlist: *expand,
                out: out.clone(),
                // **Each folder's own**, which is what `ARCHIVE_NAME` already claims an archive is:
                // a fact about *this folder*, carried with it if it is copied elsewhere. So a video
                // asked for in two folders lands in both, which is what asking for two folders
                // meant.
                archive: plan
                    .archive
                    .as_ref()
                    .map(|_| args::Plan::default_archive(out)),
                ..plan.clone()
            });
        }
    }
    Ok((plans, scratch))
}

/// The lists this tool wrote for itself, removed however [`fetch`] returns.
///
/// The same worry [`args::RECORDS_NAME`] has: for the moment they exist these sit in somebody's
/// folder of songs, and there is a real `?` between the runs — reading a record file — that would
/// otherwise leave one there for good. It cannot help a Ctrl-C, which is true of the record file too
/// and is accepted for the same reason.
///
/// **Filled as the files are written rather than from the finished plans**, which is not a
/// refactor: [`runs`] writes one file per group and has a `?` between them, so a list refused
/// halfway used to orphan everything written before it.
///
/// **[`args::ASKED_NAME`] belongs here too, and its absence was a leak rather than a nicety.** That
/// is the file a front end writes the links it was handed into; the page writes one on every Fetch
/// and nothing ever removed it, so a folder of songs collected one per download. It is never among
/// the plans [`runs`] returns for a marked list — it is the file that was *read* to make them — so
/// it is registered from the plan that came in.
#[derive(Debug, Default)]
struct Scratch(Vec<PathBuf>);

impl Scratch {
    /// Registers `path` for removal, where it is one of the three names this tool writes itself.
    ///
    /// **Matched by name, and that is load-bearing rather than convenient.** A `from_file` is just
    /// as likely to be the `km-video-fetch.kmvf` somebody maintains by hand in that same folder, and
    /// deleting a person's list at the end of a successful fetch would be the worst bug this program
    /// could have. Only what it wrote itself may be removed.
    fn writing(&mut self, path: Option<&Path>) {
        let Some(path) = path else { return };
        let name = path.file_name().unwrap_or_default();
        if [args::ASKED_NAME, args::SINGLES_NAME, args::PLAYLISTS_NAME]
            .iter()
            .any(|scratch| name == *scratch)
        {
            self.0.push(path.to_owned());
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Measures one arrived file, re-encoding it when `encoders` says to.
fn inspect(
    path: &Path,
    encoders: Option<&profile::Encoders>,
    on_event: &mut impl FnMut(Event) -> Flow,
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

    /// A folder of this test's own, so two of them cannot tread on each other.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("km-video-fetch-runs").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder to work in");
        dir
    }

    fn plan_over(out: &Path, list: &str, playlist: bool) -> args::Plan {
        let from = out.join("list.txt");
        std::fs::write(&from, list).expect("write the list");
        args::Plan {
            targets: Vec::new(),
            from_file: Some(from),
            playlist,
            out: out.to_path_buf(),
            limit: None,
            archive: Some(args::Plan::default_archive(out)),
            cookies_from_browser: None,
            subs: false,
            format: None,
            sort: None,
            dry_run: false,
            progress_lines: false,
        }
    }

    /// The compatibility guarantee, and the most important test here: a list that says nothing is
    /// handed to yt-dlp as itself, and the argv is the one this tool has always built.
    #[test]
    fn a_list_with_no_markers_makes_the_one_run_it_has_always_made() {
        let dir = scratch("plain");
        let plan = plan_over(
            &dir,
            "# a heading\n\nhttps://example.invalid/a\nhttps://example.invalid/b\n",
            false,
        );

        let (plans, scratch) = runs(&plan).expect("a plain list is one run");
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].from_file, plan.from_file,
            "the caller's own file, not a copy of it"
        );
        assert_eq!(args::argv(&plans[0]), args::argv(&plan));
        assert!(
            !dir.join(args::SINGLES_NAME).exists(),
            "and nothing was written beside it"
        );

        drop(scratch);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mixed_list_makes_one_run_of_each_kind_with_the_singles_first() {
        let dir = scratch("mixed");
        let plan = plan_over(
            &dir,
            "https://example.invalid/a\n--playlist https://example.invalid/list\n",
            false,
        );

        let (plans, scratch) = runs(&plan).expect("a mixed list splits");
        assert_eq!(plans.len(), 2);
        assert!(!plans[0].playlist, "the single videos go first");
        assert!(plans[1].playlist);
        assert_eq!(plans[0].from_file, Some(dir.join(args::SINGLES_NAME)));
        assert_eq!(plans[1].from_file, Some(dir.join(args::PLAYLISTS_NAME)));

        assert_eq!(
            std::fs::read_to_string(dir.join(args::SINGLES_NAME)).unwrap(),
            "https://example.invalid/a\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(args::PLAYLISTS_NAME)).unwrap(),
            "https://example.invalid/list\n",
            "the marker is spent by now, and yt-dlp would read it as part of the URL"
        );

        drop(scratch);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Both directions, because `--no-playlist` is the only way to say *not this one* in a run
    /// launched with `--playlist`, and a marker that only worked one way would be half a feature.
    #[test]
    fn a_marker_overrides_the_run_in_both_directions() {
        let dir = scratch("both-ways");
        let plan = plan_over(
            &dir,
            "https://example.invalid/list\n--no-playlist https://example.invalid/one\n",
            true,
        );

        let (plans, scratch) = runs(&plan).expect("a marked list splits");
        assert_eq!(plans.len(), 2);
        assert!(!plans[0].playlist, "the marked line, against the flag");
        assert!(plans[1].playlist, "and the unmarked one follows it");

        drop(scratch);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_destination_is_a_run_of_its_own_carrying_its_own_archive() {
        let dir = scratch("folders");
        let plan = plan_over(
            &dir,
            "https://example.invalid/a\n\
             --out anime https://example.invalid/b\n\
             --playlist --out anime/openings https://example.invalid/list\n",
            false,
        );

        let (plans, scratch) = runs(&plan).expect("folders split too");
        assert_eq!(plans.len(), 3);

        let outs: Vec<_> = plans.iter().map(|plan| plan.out.clone()).collect();
        assert_eq!(
            outs,
            vec![
                dir.clone(),
                dir.join("anime"),
                dir.join("anime").join("openings"),
            ],
            "singles first, then the playlist, folders in the order the list names them"
        );

        for sub in &plans {
            assert_eq!(
                sub.archive,
                Some(args::Plan::default_archive(&sub.out)),
                "each folder answers for itself what is already in it"
            );
            assert!(sub.out.is_dir(), "and the folder was made");
        }

        drop(scratch);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Refused here rather than by yt-dlp, and refused rather than quietly clamped: a list is a
    /// file, and `--out ../songs` in one copied between two machines writes into whatever happens to
    /// sit beside the destination on the second.
    #[test]
    fn a_list_cannot_name_a_folder_outside_the_one_it_was_pointed_at() {
        let dir = scratch("escape");
        let plan = plan_over(&dir, "--out ../beside https://example.invalid/a\n", false);
        assert!(runs(&plan).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The generated lists are this function's own business and must not outlive it.
    #[test]
    fn the_lists_a_run_wrote_for_itself_are_taken_away_again() {
        let dir = scratch("scratch");
        let plan = plan_over(
            &dir,
            "https://example.invalid/a\n--playlist https://example.invalid/list\n",
            false,
        );

        let (_plans, scratch) = runs(&plan).expect("a mixed list splits");
        assert!(dir.join(args::SINGLES_NAME).exists());
        drop(scratch);
        assert!(!dir.join(args::SINGLES_NAME).exists());
        assert!(!dir.join(args::PLAYLISTS_NAME).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression. The file a front end writes the links into is scratch too, and nothing used to
    /// remove it: the page wrote one on every Fetch and a folder of songs collected one per
    /// download. Both shapes matter — for a marked list the asked file is not among the plans that
    /// come back, it is what was read to make them, so it can only be caught from the plan going in.
    #[test]
    fn the_list_a_front_end_wrote_goes_away_too_however_the_run_was_split() {
        for (name, list) in [
            ("asked-plain", "https://example.invalid/a\n"),
            (
                "asked-split",
                "https://example.invalid/a\n--playlist https://example.invalid/list\n",
            ),
        ] {
            let dir = scratch(name);
            let asked = dir.join(args::ASKED_NAME);
            std::fs::write(&asked, list).expect("write the list");
            let plan = args::Plan {
                from_file: Some(asked.clone()),
                ..plan_over(&dir, list, false)
            };

            let (_plans, scratch) = runs(&plan).expect("either shape is a run");
            assert!(asked.exists(), "still there while the fetch is happening");
            drop(scratch);
            assert!(!asked.exists(), "and gone when it is over ({name})");

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// **The most important assertion about `Scratch`.** A `from_file` is just as likely to be the
    /// list somebody maintains by hand in that same folder, and deleting it at the end of a
    /// successful fetch would be the worst bug this program could have. The filter is by name, and
    /// this is what holds it to that.
    #[test]
    fn a_list_somebody_maintains_by_hand_is_never_removed() {
        let dir = scratch("hand-written");
        for name in [args::BATCH_NAME, "list.txt", "songs.kmvf"] {
            let own = dir.join(name);
            std::fs::write(&own, "https://example.invalid/a\n").expect("write the list");
            let plan = args::Plan {
                from_file: Some(own.clone()),
                ..plan_over(&dir, "https://example.invalid/a\n", false)
            };

            let (_plans, scratch) = runs(&plan).expect("one run");
            drop(scratch);
            assert!(
                own.exists(),
                "a file this did not write is not this function's to remove ({name})"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
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
