//! Finding yt-dlp, running it, and reading back what it did.
//!
//! # Where the binary comes from
//!
//! `PATH`, by the same reasoning [`crate::profile`] finds ffmpeg on `PATH` and neither bundles nor
//! configures it: this is a tool run by whoever is curating a corpus, on their machine, and it is their yt-dlp.
//! Unlike ffmpeg it takes a `--yt-dlp` override, because yt-dlp is very often installed by `pipx` or
//! into a virtualenv and is genuinely often *not* on `PATH` even on a machine that has it.
//!
//! # Why the child keeps the terminal, where there is one
//!
//! yt-dlp's progress output is better than anything this could reconstruct from a pipe, so [`spawn`]
//! inherits stdout and stderr and it draws straight to the terminal. The machine-readable half goes
//! to a file instead, via `--print-to-file`. That is not merely simpler than piping — it removes the
//! failure the ffmpeg call in [`crate::profile`] has to spawn a thread to avoid, where a child fills
//! a pipe nobody is draining and both processes stop.
//!
//! **A web page has no terminal to hand over**, so [`spawn_watched`] pipes after all — and it is
//! therefore the one function here that has to answer the paragraph above rather than benefit from
//! it. It does so the same way `profile::transcode` does: **each pipe is drained by a thread of its
//! own**, so neither can fill while nothing reads it. The caller's line sink runs on the stdout
//! thread. Nothing about [`spawn`] changes; the two exist side by side because the choice is real.
//!
//! # The console window is the second half of that same choice
//!
//! On Windows a child of a GUI-subsystem parent is given a console window of its own unless it
//! is told otherwise, and `km-video-downloader` is such a parent — so every call in this module
//! but one carries [`crate::child::without_a_console_window`]. [`spawn`] is the exception, and
//! for the same reason it inherits in the first place: the flag would take away the console it
//! is being handed. That module states the rule the four call sites here are decided by.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::child::without_a_console_window;

/// What yt-dlp is called when nothing says otherwise.
pub const YT_DLP: &str = "yt-dlp";

/// How old a yt-dlp may be before it is worth mentioning, in days.
///
/// YouTube changes what it serves and yt-dlp follows; a copy from last year fails with signature
/// and format errors that read like a broken network or a removed video. Saying so up front turns a
/// confusing half hour into a `pipx upgrade`.
pub const STALE_AFTER_DAYS: i64 = 90;

/// The binary to run: what was asked for, or the one on `PATH`.
#[must_use]
pub fn binary(override_path: Option<&Path>) -> OsString {
    override_path.map_or_else(
        || OsString::from(YT_DLP),
        |path| path.as_os_str().to_owned(),
    )
}

/// Asks yt-dlp its version, which doubles as proving it can be run at all.
pub fn version(binary: &OsStr) -> Result<String> {
    let output = without_a_console_window(&mut Command::new(binary))
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!(
                "could not run `{}` — install yt-dlp and put it on the PATH, or pass --yt-dlp",
                binary.to_string_lossy()
            )
        })?;

    if !output.status.success() {
        bail!(
            "`{} --version` exited with {}",
            binary.to_string_lossy(),
            output.status
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// How many days old a `YYYY.MM.DD` version string is, when it looks like one.
///
/// `None` rather than an error for anything unparseable: a nightly build calls itself
/// `2026.08.20.232134` and a distribution patch may call itself anything at all, and refusing to run
/// over a version string would be a tool that stops working for a cosmetic reason.
#[must_use]
pub fn age_in_days(version: &str, today: i64) -> Option<i64> {
    let mut parts = version.split('.');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(today - days_from_civil(year, month, day))
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
///
/// Howard Hinnant's `days_from_civil`, which is the standard way to do this without a calendar
/// crate. Worth the fifteen lines: the alternative is a dependency the workspace does not otherwise
/// have, carried for one comparison against one date.
#[must_use]
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Today, as days since 1970-01-01.
#[must_use]
pub fn today() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs() / 86_400).unwrap_or(0)
        })
}

/// Checks that the ffmpeg yt-dlp needs is reachable.
///
/// Needed for two of the things asked of it, not one: merging the separate video and audio streams
/// YouTube serves, and writing the container tags the whole metadata chain depends on. Without it
/// yt-dlp silently produces a `.webm` beside a `.m4a` and reports success, which is a far worse
/// outcome than being told now.
pub fn ensure_ffmpeg() -> Result<()> {
    without_a_console_window(&mut Command::new("ffmpeg"))
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context(
            "could not run `ffmpeg` — yt-dlp needs it to merge streams and to write metadata, and \
             `ffprobe` from the same package is what reads the result back. Install it with your \
             package manager, winget or brew.",
        )?;
    Ok(())
}

/// Runs yt-dlp with the terminal attached, returning whether it succeeded.
///
/// A non-zero exit is reported rather than raised: over a playlist, one video that is private or
/// region-locked fails the whole run, and everything else in it still downloaded. The caller says
/// so and goes on to check what did arrive.
pub fn spawn(binary: &OsStr, args: &[OsString]) -> Result<bool> {
    // **The one call here without [`crate::child::without_a_console_window`]**, and the only one
    // that inherits rather than captures. See that module: the flag detaches a child from the
    // console it was given, which is the very thing this function exists to hand over.
    let status = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("running `{}`", binary.to_string_lossy()))?;
    Ok(status.success())
}

/// Whether to keep going.
///
/// Returned by the line sink rather than read from a flag the caller also holds, so that the one
/// thing already being called for every line is also the thing that can say stop. There is nowhere
/// else to ask: between lines is the only moment this function is not blocked on a pipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Carry on.
    Go,
    /// Stop as soon as possible.
    Stop,
}

/// Runs yt-dlp with its output piped, calling `on_line` for every line of it.
///
/// **`on_line` also says whether to carry on**, by returning a [`Flow`]. A caller with a Stop
/// button has nowhere else to be asked: between lines is the only moment this function is not
/// blocked on a pipe.
///
/// The same contract as [`spawn`] — a non-zero exit is reported rather than raised — and the same
/// return value. What differs is where yt-dlp's words go: to `on_line` rather than to a terminal.
///
/// **Both pipes are drained.** A child whose stderr fills while nothing reads it stops, and so does
/// the parent waiting on it; "the download hangs on some videos" is not a bug worth discovering
/// later.
///
/// The split of labour is the one `profile::transcode` already uses: **stdout is read on this
/// thread** and stderr on a thread of its own that only
/// collects. That is what keeps `on_line` off any thread but the caller's — so it need not be
/// `Send`, and a caller may hand over a closure holding whatever it likes. The price is that
/// stderr arrives in a block at the end rather than interleaved; with `--newline` in effect
/// yt-dlp's progress and nearly all of its narration are on stdout, and what stderr carries is the
/// errors, which is exactly the part somebody reads afterwards.
///
/// Lines are what a caller gets rather than bytes, because yt-dlp is asked for `--newline` and the
/// progress template writes whole lines too. A line that is not valid UTF-8 is lossily converted
/// rather than dropped: a video with an unusual title is exactly the one somebody is watching for.
pub fn spawn_watched(
    binary: &OsStr,
    args: &[OsString],
    mut on_line: impl FnMut(&str) -> Flow,
) -> Result<bool> {
    let mut child = without_a_console_window(&mut Command::new(binary))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("running `{}`", binary.to_string_lossy()))?;

    let stderr = child.stderr.take();
    let collect = std::thread::spawn(move || lines_of(stderr));

    // **Read as it arrives, not collected first.** stdout is where the progress is, and a caller
    // drawing a bar from it needs the lines now rather than at the end.
    let mut stopped = false;
    if let Some(stdout) = child.stdout.take() {
        use std::io::{BufRead as _, BufReader};
        for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line).trim_end().to_owned();
            if text.is_empty() {
                continue;
            }
            if on_line(&text) == Flow::Stop {
                // **Killed, because there is no gentler way.** yt-dlp downloads a whole playlist in
                // one process, so stopping means ending it. What it leaves behind is a `.part` file,
                // and `--continue` — which this always passes — picks that up next time rather than
                // starting the video again.
                let _ = child.kill();
                stopped = true;
                break;
            }
        }
    }

    let status = child.wait().context("waiting for yt-dlp")?;
    for line in collect.join().unwrap_or_default() {
        on_line(&line);
    }
    // A killed child exits unsuccessfully, and reporting that as a failure would put "yt-dlp
    // reported a failure" on the screen of somebody who pressed Stop.
    Ok(stopped || status.success())
}

/// Every non-empty line a reader produces, lossily decoded.
fn lines_of(reader: Option<impl std::io::Read>) -> Vec<String> {
    use std::io::{BufRead, BufReader};

    let Some(reader) = reader else {
        return Vec::new();
    };
    BufReader::new(reader)
        .split(b'\n')
        .map_while(Result::ok)
        .map(|bytes| String::from_utf8_lossy(&bytes).trim_end().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// One line of `--print-to-file`: what yt-dlp knew about one video.
///
/// Every field is optional because every field genuinely can be missing — `%(...)j` writes JSON
/// `null` for anything the extractor did not report, and a plain YouTube upload reports neither
/// `artist` nor `track`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Record {
    /// The extractor's id for the video.
    pub id: Option<String>,
    /// Where the file ended up. Absent on a dry run, which never writes one.
    pub filepath: Option<String>,
    /// The music artist, when the extractor reported the upload as music.
    pub artist: Option<String>,
    /// The album artist, sometimes present when `artist` is not.
    pub album_artist: Option<String>,
    /// The older spelling of the same idea.
    pub creator: Option<String>,
    /// The channel, which is the last thing worth calling an artist and often the wrong one.
    pub uploader: Option<String>,
    /// The song title, when the extractor reported the upload as music.
    pub track: Option<String>,
    /// The video's title, which is what most uploads have instead.
    pub title: Option<String>,
    /// The page it came from.
    pub webpage_url: Option<String>,
    /// Length in seconds.
    pub duration: Option<f64>,
}

impl Record {
    /// The best title available, in the order the output template prefers them.
    #[must_use]
    pub fn best_title(&self) -> Option<&str> {
        first_present(&[self.track.as_deref(), self.title.as_deref()])
    }

    /// The best artist available, in the order the output template prefers them.
    #[must_use]
    pub fn best_artist(&self) -> Option<&str> {
        first_present(&[
            self.artist.as_deref(),
            self.album_artist.as_deref(),
            self.creator.as_deref(),
            self.uploader.as_deref(),
        ])
    }

    /// Where the file landed, when one was written.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        self.filepath.as_ref().map(PathBuf::from)
    }

    /// A one-line description for the summary.
    ///
    /// Falls back through the id to the page it came from, because a video that reported no title
    /// at all is exactly the one somebody will want to go and look at.
    #[must_use]
    pub fn describe(&self) -> String {
        match (self.best_artist(), self.best_title()) {
            (Some(artist), Some(title)) => format!("{artist} - {title}"),
            (None, Some(title)) => title.to_owned(),
            _ => self
                .id
                .clone()
                .or_else(|| self.webpage_url.clone())
                .unwrap_or_else(|| "an unnamed video".to_owned()),
        }
    }

    /// The length as `m:ss`, or `h:mm:ss` past an hour.
    ///
    /// Shown on a dry run, where it is the one number that says whether a result is the song or
    /// somebody's ninety-minute upload of a whole album.
    #[must_use]
    pub fn length(&self) -> Option<String> {
        let seconds = self.duration.filter(|value| *value > 0.0)?;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "guarded positive above, and a duration past u64 is not a song"
        )]
        let total = seconds as u64;
        let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
        Some(if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes}:{seconds:02}")
        })
    }
}

/// The first of `values` that is present and not blank.
fn first_present<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty() && *value != "NA")
}

/// Reads the records yt-dlp wrote, one JSON object per line.
///
/// A missing file is an empty list rather than an error: yt-dlp creates it only when it has
/// something to print, so a run where every url was already in the archive legitimately leaves
/// nothing behind. A line that will not parse is skipped rather than fatal — the files it describes
/// were still downloaded, and losing the summary is not worth losing them over.
pub fn read_records(path: &Path) -> Result<Vec<Record>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    Ok(parse_records(&text))
}

/// Parses the record file's contents.
#[must_use]
pub fn parse_records(text: &str) -> Vec<Record> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Record>(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_prefers_the_music_fields_over_the_channel() {
        let records = parse_records(
            r#"{"id":"a","filepath":"videos/Howl - Aeng Moo Sae.mp4","artist":"Howl","album_artist":null,"creator":null,"uploader":"SomeChannel","track":"Aeng Moo Sae","title":"[MV] Howl - Aeng Moo Sae","webpage_url":"https://x/a","duration":273.02}"#,
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].best_artist(), Some("Howl"));
        assert_eq!(records[0].best_title(), Some("Aeng Moo Sae"));
        assert_eq!(records[0].describe(), "Howl - Aeng Moo Sae");
        assert_eq!(
            records[0].path(),
            Some(PathBuf::from("videos/Howl - Aeng Moo Sae.mp4"))
        );
    }

    /// The ordinary case, and the reason the output template has a conditional in it: an upload
    /// that is not tagged as music has no artist at all, and the channel is a poor stand-in that is
    /// nevertheless better than nothing.
    #[test]
    fn a_plain_upload_falls_back_to_the_channel_and_the_video_title() {
        let records = parse_records(
            r#"{"id":"b","filepath":"videos/A Song.mp4","artist":null,"track":null,"title":"A Song","uploader":"Karaoke Channel"}"#,
        );
        assert_eq!(records[0].best_artist(), Some("Karaoke Channel"));
        assert_eq!(records[0].best_title(), Some("A Song"));
    }

    #[test]
    fn a_record_with_nothing_in_it_still_describes_itself() {
        let records = parse_records(r#"{"id":"c"}"#);
        assert_eq!(records[0].describe(), "c");
        assert_eq!(records[0].path(), None);
    }

    /// yt-dlp writes the literal string `NA` for some absent fields rather than a JSON null, and a
    /// song by `NA` is worse than a song by nobody.
    #[test]
    fn the_literal_na_is_not_an_artist() {
        let records = parse_records(r#"{"id":"d","artist":"NA","uploader":"  ","title":"T"}"#);
        assert_eq!(records[0].best_artist(), None);
    }

    #[test]
    fn blank_and_broken_lines_are_skipped_rather_than_fatal() {
        let records = parse_records("\n{\"id\":\"a\"}\nnot json at all\n\n{\"id\":\"b\"}\n");
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn the_epoch_and_a_known_date_agree() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 8, 26), 20_691);
    }

    #[test]
    fn a_version_reports_its_age_and_a_nightly_declines_to() {
        let today = days_from_civil(2026, 8, 26);
        assert_eq!(age_in_days("2026.08.26", today), Some(0));
        assert_eq!(age_in_days("2026.05.28", today), Some(90));
        assert_eq!(age_in_days("2025.08.26", today), Some(365));

        // Extra components are ignored rather than rejected: a nightly still starts with a date.
        assert_eq!(age_in_days("2026.08.26.232134", today), Some(0));

        assert_eq!(age_in_days("not-a-version", today), None);
        assert_eq!(age_in_days("2026.13.01", today), None);
        assert_eq!(age_in_days("2026", today), None);
    }

    #[test]
    fn the_binary_is_yt_dlp_unless_told_otherwise() {
        assert_eq!(binary(None), OsString::from("yt-dlp"));
        assert_eq!(
            binary(Some(Path::new("/opt/bin/yt-dlp"))),
            OsString::from("/opt/bin/yt-dlp")
        );
    }
}
