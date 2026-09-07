//! One fetch, running in the background, and what the page is told about it.
//!
//! # The shape, and why it is this one
//!
//! A `Job` is shared as an `Arc` between the worker that fills it in and the request handler that
//! reads it. Every field is an atomic or a small mutex, and [`Job::view`] takes a snapshot **field
//! by field rather than under one lock**. That is deliberate: a single lock over the whole job would
//! make the download wait on a browser poll, and the worst it costs is a bar one tick out of step
//! with its own label, which nobody can see.
//!
//! # `total == 0` means *not known yet*
//!
//! Never *nothing to do*. Until yt-dlp has said how many videos a playlist holds, the bar is drawn
//! as working rather than as 0%, and the numbers are left off entirely — because a bar sitting at
//! zero reads as a program that has failed to start.
//!
//! # Stopping is asking
//!
//! There is no way to kill yt-dlp mid-download that leaves a folder in a state anybody wants, so
//! [`Job::ask_to_stop`] sets a flag that is read between videos. The page says *stopping* rather
//! than *stopped*, because that is what is true.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use km_video_core::fetch;

/// How many lines of yt-dlp's own output to keep.
///
/// **A cap rather than everything**, because a playlist of four hundred videos writes tens of
/// thousands of lines and all of them would be re-rendered into the page once a second. The last
/// few hundred are what a person reads when something has gone wrong; the rest is scrollback nobody
/// asked for.
const LOG_LINES: usize = 400;

/// One fetch in progress or lately finished.
#[derive(Debug)]
pub struct Job {
    /// What is happening, in words a person is shown.
    phase: Mutex<String>,
    /// How far along the *current* download is, 0 to 100.
    percent: AtomicU64,
    /// Videos finished, and how many there are altogether. Zero total means not known yet.
    done: AtomicU64,
    total: AtomicU64,
    /// yt-dlp's own output, capped at [`LOG_LINES`].
    log: Mutex<Vec<String>>,
    /// Each file that arrived, and what it turned out to be.
    results: Mutex<Vec<Arrival>>,
    /// Somebody has pressed Stop.
    cancel: AtomicBool,
    /// The work has ended, one way or the other.
    finished: AtomicBool,
    /// What went wrong, where something did.
    error: Mutex<Option<String>>,
    /// What happened, where it worked.
    outcome: Mutex<Option<String>>,
    started: Instant,
}

/// One file that landed, flattened into what the page draws.
#[derive(Debug, Clone)]
pub struct Arrival {
    /// The file's own name, or a description of the video where no file was written.
    pub name: String,
    /// What the profile made of it, in one sentence. Empty where there is nothing to say.
    pub verdict: String,
    /// How to colour it: `ok`, `warn` or `bad`.
    pub tone: &'static str,
}

impl Job {
    /// A new job, not yet started.
    #[must_use]
    pub fn new(phase: impl Into<String>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            phase: Mutex::new(phase.into()),
            percent: AtomicU64::new(0),
            done: AtomicU64::new(0),
            total: AtomicU64::new(0),
            log: Mutex::new(Vec::new()),
            results: Mutex::new(Vec::new()),
            cancel: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            error: Mutex::new(None),
            outcome: Mutex::new(None),
            started: Instant::now(),
        })
    }

    /// Folds one of [`fetch::fetch`]'s events into what the page shows.
    ///
    /// **This is the whole of the translation**, and it is the counterpart of the command line's
    /// `Reporter`: the same events, turned into a bar and a list instead of into lines.
    pub fn absorb(&self, event: &fetch::Event) {
        use fetch::Event as E;
        match event {
            E::Tool {
                version,
                stale_days,
            } => {
                self.say(&format!("yt-dlp {version}"));
                if let Some(age) = stale_days
                    && *age > km_video_core::run::STALE_AFTER_DAYS
                {
                    self.say(&format!(
                        "warning: that yt-dlp is {age} days old. Sites change what they serve and \
                         yt-dlp follows; a stale copy fails in ways that look like a broken \
                         network."
                    ));
                }
            }
            E::ReadingList(list) => self.say(&format!("reading {}", list.display())),
            E::Command(line) => {
                self.phase("asking yt-dlp");
                self.say(line);
            }
            E::Said(line) => self.say(line),

            E::Downloading {
                title,
                percent,
                speed,
                eta,
            } => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the parser clamps this to 0..=100"
                )]
                self.percent.store(*percent as u64, Ordering::Relaxed);
                let mut phase = format!("downloading {title}");
                if let Some(speed) = speed {
                    phase.push_str(&format!(" · {speed}"));
                }
                if let Some(eta) = eta {
                    phase.push_str(&format!(" · {eta} left"));
                }
                self.phase(&phase);
            }

            E::Downloaded { ok, fetched } => {
                if !ok {
                    self.say("yt-dlp reported a failure; what did arrive is below.");
                }
                self.total.store(*fetched as u64, Ordering::Relaxed);
                self.percent.store(0, Ordering::Relaxed);
                self.phase("checking what arrived");
            }

            E::Normalizing { path, percent } => {
                self.percent.store(u64::from(*percent), Ordering::Relaxed);
                self.phase(&format!("re-encoding {}", name_of(path)));
            }

            E::Arrived { record, verdict } => {
                self.done.fetch_add(1, Ordering::Relaxed);
                self.percent.store(0, Ordering::Relaxed);
                self.results
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(arrival(record, verdict));
            }
        }
    }

    /// Sets the phase, which is the line under the bar.
    pub fn phase(&self, phase: &str) {
        let mut slot = self.phase.lock().unwrap_or_else(|p| p.into_inner());
        if *slot != phase {
            phase.clone_into(&mut slot);
        }
    }

    /// Adds one line to the log, dropping the oldest once it is full.
    pub fn say(&self, line: &str) {
        let mut log = self.log.lock().unwrap_or_else(|p| p.into_inner());
        if log.len() >= LOG_LINES {
            log.remove(0);
        }
        log.push(line.to_owned());
    }

    /// Asks the run to stop after the video it is on.
    pub fn ask_to_stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Whether somebody has asked it to stop.
    #[must_use]
    pub fn stopping(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Ends the job, saying what came of it.
    pub fn done_with(&self, outcome: impl Into<String>) {
        *self.outcome.lock().unwrap_or_else(|p| p.into_inner()) = Some(outcome.into());
        self.finished.store(true, Ordering::Relaxed);
    }

    /// Ends the job, saying what went wrong.
    pub fn failed_with(&self, why: impl Into<String>) {
        *self.error.lock().unwrap_or_else(|p| p.into_inner()) = Some(why.into());
        self.finished.store(true, Ordering::Relaxed);
    }

    /// Everything the page needs, read once.
    #[must_use]
    pub fn view(&self) -> View {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        View {
            phase: self.phase.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            percent: self.percent.load(Ordering::Relaxed),
            done,
            total,
            running: !self.finished.load(Ordering::Relaxed),
            stopping: self.stopping(),
            error: self.error.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            outcome: self
                .outcome
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            elapsed_secs: self.started.elapsed().as_secs(),
            log: self.log.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        }
    }

    /// What arrived so far.
    #[must_use]
    pub fn results(&self) -> Vec<Arrival> {
        self.results
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

/// A snapshot of a job, which is what a template is handed.
#[derive(Debug, Clone)]
pub struct View {
    /// What is happening, in words.
    pub phase: String,
    /// How far along the current item is, 0 to 100.
    pub percent: u64,
    /// Items finished.
    pub done: u64,
    /// Items altogether. Zero means not known yet — see this module's header.
    pub total: u64,
    /// Whether it is still going.
    pub running: bool,
    /// Whether a stop has been asked for and not yet taken effect.
    pub stopping: bool,
    /// What went wrong, where something did.
    pub error: Option<String>,
    /// What came of it, where it worked.
    pub outcome: Option<String>,
    /// How long it has been going.
    pub elapsed_secs: u64,
    /// yt-dlp's own words.
    pub log: Vec<String>,
}

impl View {
    /// Whether the total is not known yet, so the bar should sweep rather than fill.
    #[must_use]
    pub fn indeterminate(&self) -> bool {
        self.total == 0
    }

    /// `2 of 7`, where that is known.
    #[must_use]
    pub fn counted(&self) -> Option<String> {
        (self.total > 0).then(|| format!("{} of {}", self.done, self.total))
    }

    /// Whether yt-dlp's own words are the thing to read next.
    ///
    /// **Opened rather than folded away**, because a run that ended with nothing to show has put
    /// its explanation in the log and nowhere else. A run that fetched something has the list of
    /// what arrived to look at instead, and the log is scrollback.
    #[must_use]
    pub fn log_matters(&self) -> bool {
        !self.running && (self.error.is_some() || self.total == 0)
    }
}

/// One arrived file, as the page shows it.
fn arrival(record: &km_video_core::run::Record, verdict: &fetch::Verdict) -> Arrival {
    use fetch::Verdict as V;

    // **Settled by the verdict, before anything looks for a file**, because a dry run never writes
    // one: `args::SIMULATE_TEMPLATE` leaves `filepath` out on purpose, there being nothing to put in
    // it. Asking for the path first is what made every row of a dry run a red *no file was written*
    // here, while the command line said *would fetch this (3:33)* about the same record — and it
    // left the `NotFetched` arm below unreachable from this page.
    if let V::NotFetched = verdict {
        return Arrival {
            name: record.describe(),
            verdict: record
                .length()
                .map_or_else(String::new, |length| format!("would fetch this ({length})")),
            tone: "ok",
        };
    }

    // A run that meant to write a file and recorded none. That is a fault, and the row says so.
    let Some(path) = record.path() else {
        return Arrival {
            name: record.describe(),
            verdict: "no file was written".to_owned(),
            tone: "bad",
        };
    };

    let (verdict, tone) = match verdict {
        V::InProfile => ("in profile — packaging will copy it".to_owned(), "ok"),
        V::Outside { summary } => (
            format!("outside the profile: {summary} — packaging will re-encode it"),
            "warn",
        ),
        V::Unplayable { summary } => (format!("cannot be played as it is: {summary}"), "bad"),
        V::Normalized { summary } => (format!("re-encoded into profile ({summary})"), "ok"),
        V::Unreadable { why } => (format!("could not be probed: {why}"), "bad"),
        // Answered above, before a file was looked for. Spelled out rather than left to an
        // `unreachable!`, which would be a panic in a page renderer to save four words.
        V::NotFetched => (String::new(), "ok"),
    };

    Arrival {
        name: name_of(&path),
        verdict,
        tone,
    }
}

/// A path's last component, or the whole thing where it has none.
fn name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_record(path: Option<&str>) -> km_video_core::run::Record {
        km_video_core::run::Record {
            filepath: path.map(str::to_owned),
            title: Some("A Song".to_owned()),
            ..km_video_core::run::Record::default()
        }
    }

    /// The rule the bar is drawn from, asserted where it is decided rather than in a template.
    #[test]
    fn an_unknown_total_is_not_a_total_of_none() {
        let job = Job::new("starting");
        assert!(job.view().indeterminate(), "nothing known yet");
        assert_eq!(job.view().counted(), None, "and no numbers are offered");

        job.absorb(&fetch::Event::Downloaded {
            ok: true,
            fetched: 3,
        });
        assert!(!job.view().indeterminate());
        assert_eq!(job.view().counted().as_deref(), Some("0 of 3"));
    }

    #[test]
    fn each_arrival_is_counted_and_coloured() {
        let job = Job::new("starting");
        job.absorb(&fetch::Event::Downloaded {
            ok: true,
            fetched: 2,
        });

        job.absorb(&fetch::Event::Arrived {
            record: a_record(Some("videos/A Song.mp4")),
            verdict: fetch::Verdict::InProfile,
        });
        job.absorb(&fetch::Event::Arrived {
            record: a_record(Some("videos/Other.mkv")),
            verdict: fetch::Verdict::Unplayable {
                summary: "yuv444p video".to_owned(),
            },
        });

        let results = job.results();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].name, "A Song.mp4",
            "the name, not the whole path"
        );
        assert_eq!(results[0].tone, "ok");
        assert_eq!(results[1].tone, "bad");
        assert_eq!(job.view().counted().as_deref(), Some("2 of 2"));
    }

    /// A video that produced no file still gets a row, because a list with a gap in it looks like a
    /// clean run that was one video shorter.
    #[test]
    fn a_video_with_no_file_still_gets_a_row() {
        let job = Job::new("starting");
        job.absorb(&fetch::Event::Arrived {
            record: a_record(None),
            verdict: fetch::Verdict::Unreadable {
                why: "whatever".to_owned(),
            },
        });
        let results = job.results();
        assert_eq!(results[0].name, "A Song");
        assert_eq!(results[0].verdict, "no file was written");
        assert_eq!(results[0].tone, "bad");
    }

    /// The regression. A dry run writes no file *by design* — `SIMULATE_TEMPLATE` leaves `filepath`
    /// out because there is nothing to put in it — so a row settled by looking for the path made
    /// every line of a dry run a red fault on this page, while the command line was calling the same
    /// record something it would fetch. What the row says is the verdict's to decide.
    #[test]
    fn a_dry_run_says_what_it_would_fetch_rather_than_that_nothing_arrived() {
        let job = Job::new("starting");
        job.absorb(&fetch::Event::Arrived {
            record: km_video_core::run::Record {
                duration: Some(213.0),
                ..a_record(None)
            },
            verdict: fetch::Verdict::NotFetched,
        });
        let results = job.results();
        assert_eq!(results[0].name, "A Song");
        assert_eq!(results[0].verdict, "would fetch this (3:33)");
        assert_eq!(results[0].tone, "ok", "a dry run is not a failure");
    }

    /// A video whose length the extractor did not report is still a row, and still not a fault.
    #[test]
    fn a_dry_run_row_survives_a_video_of_unknown_length() {
        let job = Job::new("starting");
        job.absorb(&fetch::Event::Arrived {
            record: a_record(None),
            verdict: fetch::Verdict::NotFetched,
        });
        let results = job.results();
        assert_eq!(results[0].name, "A Song");
        assert_eq!(results[0].verdict, "");
        assert_eq!(results[0].tone, "ok");
    }

    /// Four hundred videos is tens of thousands of lines, and all of them would be re-rendered into
    /// the page once a second.
    #[test]
    fn the_log_keeps_the_last_lines_rather_than_all_of_them() {
        let job = Job::new("starting");
        for n in 0..(LOG_LINES + 50) {
            job.say(&format!("line {n}"));
        }
        let log = job.view().log;
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log[0], "line 50", "the oldest went");
        assert_eq!(log[LOG_LINES - 1], format!("line {}", LOG_LINES + 49));
    }

    #[test]
    fn a_finished_job_is_not_running() {
        let job = Job::new("starting");
        assert!(job.view().running);
        job.done_with("fetched 2 videos");
        let view = job.view();
        assert!(!view.running);
        assert_eq!(view.outcome.as_deref(), Some("fetched 2 videos"));
        assert_eq!(view.error, None);
    }
}
