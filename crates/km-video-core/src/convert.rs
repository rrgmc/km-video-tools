//! A video already on disk, brought into the shape packaging wants.
//!
//! # What this is, beside [`crate::fetch`]
//!
//! The same second half of a fetch, over a file nobody downloaded. [`crate::fetch`] asks yt-dlp for a
//! video and then measures what landed; this measures a file that was already there. Both end in
//! [`crate::profile`], and neither decides anything about wording.
//!
//! So the events are shaped like that module's, for the reason the split there exists: one command
//! line renders them as printed lines and one page renders them as a bar, and neither front end
//! knows what the other looks like.
//!
//! # The source is read and never written
//!
//! [`crate::check::normalize`] replaces the file it re-encoded. That is right for something yt-dlp
//! wrote a second ago and wrong for a file somebody owns, so nothing here touches the source: the
//! result is a new file in the output folder, and what was named is still where it was.
//!
//! # A file already in the shape packaging wants is copied
//!
//! Re-encoding it would spend minutes and a generation of picture to produce a file the packager
//! treats exactly as it would have treated the one that went in. The output folder is the whole
//! result of a run, so such a file still lands in it, by a copy.
//!
//! # A size asked for is a reason to re-encode here
//!
//! The opposite of [`crate::fetch`]'s rule, and for a reason that does not carry over. There a size
//! is a request to a site, and a file that arrives larger is left alone rather than charged an hour
//! of CPU nobody asked for. Here the re-encode *is* what was asked for, so a picture larger than the
//! size named is one of the things this run exists to put right.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::{check, profile, run, size};

/// Whether to keep going, said by the same closure that hears about everything else.
pub use crate::run::Flow;

/// The file extensions a folder is searched for.
///
/// A list rather than *whatever ffprobe will open*, because a folder of songs also holds the
/// archive, a list of links, cover art and whatever else somebody keeps beside their videos, and
/// finding out by running ffprobe over each of them is a subprocess apiece.
///
/// A file named on its own is taken whatever it is called. Somebody who names a file has said which
/// file they mean.
pub const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mkv", "webm", "mov", "avi", "m4v"];

/// Whether this path carries one of the extensions in [`VIDEO_EXTENSIONS`].
#[must_use]
pub fn looks_like_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// Everything a conversion depends on.
#[derive(Debug, Clone)]
pub struct Request {
    /// What to convert. Each is a file, or a folder to take the videos in it.
    pub sources: Vec<PathBuf>,
    /// Where the converted files go.
    pub out: PathBuf,
    /// How much picture to keep. `None` is the largest size packaging accepts.
    pub video: Option<size::Video>,
    /// Say what would happen, and change nothing.
    pub dry_run: bool,
}

/// One thing that happened, as it happened.
#[derive(Debug, Clone)]
pub enum Event {
    /// How many files the sources came to, before any of them is read.
    ///
    /// **Carries the count**, which is what lets a caller draw a bar over the run rather than one
    /// that sweeps until the last file is done.
    Found {
        /// How many files will be looked at.
        count: usize,
    },
    /// The ffmpeg command about to run, quoted well enough to paste into a shell.
    ///
    /// One per file re-encoded, the input, the output and the filters all being that file's own.
    Command(String),
    /// A re-encode is running. Emitted repeatedly for one file.
    Converting {
        /// The file being read.
        source: PathBuf,
        /// 0 to 100.
        percent: u8,
    },
    /// One file has been dealt with, and this is what came of it.
    Done {
        /// The file that was named.
        source: PathBuf,
        /// What was made of it.
        verdict: Verdict,
    },
}

/// What became of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Already what packaging wants, and copied into the output folder as it is.
    Copied {
        /// Where it landed.
        destination: PathBuf,
    },
    /// Re-encoded into the output folder.
    Converted {
        /// Where it landed.
        destination: PathBuf,
        /// What was wrong with it, joined for reading.
        summary: String,
    },
    /// A dry run: it is already what packaging wants.
    WouldCopy {
        /// Where it would land.
        destination: PathBuf,
    },
    /// A dry run: this is what would be put right.
    WouldConvert {
        /// Where it would land.
        destination: PathBuf,
        /// What is wrong with it, joined for reading.
        summary: String,
    },
    /// Something is in the output folder under that name already, and nothing was written.
    ///
    /// **A refusal rather than an overwrite.** The output folder is somebody's library. This is also
    /// what stops a file that is its own destination being ffmpeg's input and output at once.
    Exists {
        /// The name that is taken.
        destination: PathBuf,
    },
    /// The file is on disk and could not be read.
    ///
    /// **A finding rather than a failure**, so a run goes on to the next file. One unreadable video
    /// must not cost somebody the other nineteen.
    Unreadable {
        /// What went wrong, worded for a person.
        why: String,
    },
    /// The copy or the re-encode failed.
    Failed {
        /// What went wrong, worded for a person.
        why: String,
    },
}

impl Verdict {
    /// Whether this file produced nothing in the output folder.
    #[must_use]
    pub fn refused(&self) -> bool {
        matches!(
            self,
            Self::Exists { .. } | Self::Unreadable { .. } | Self::Failed { .. }
        )
    }
}

/// How a whole conversion came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// How many files were re-encoded.
    pub converted: usize,
    /// How many were already in the profile and were copied.
    pub copied: usize,
    /// How many produced no file in the output folder.
    pub refused: usize,
    /// Whether the caller asked it to stop before it was done.
    pub stopped: bool,
}

/// What one file needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    /// Nothing to put right: its bytes are what should land in the output folder.
    Copy,
    /// Re-encode, and this is what is wrong with it.
    Encode {
        /// The findings, joined for reading.
        summary: String,
    },
}

/// Runs one conversion from end to end.
///
/// Fails only for the things that stop a run before it starts — no ffmpeg, nothing named, a source
/// that is not there, an output folder that cannot be made. Everything after that is reported as an
/// event and summarised in the [`Outcome`], because by then files are being written and a caller
/// needs to hear about them rather than about an error.
pub fn convert(request: &Request, mut on_event: impl FnMut(Event) -> Flow) -> Result<Outcome> {
    if request.sources.is_empty() {
        bail!("nothing to convert — name a video file, or a folder with videos in it");
    }
    run::ensure_ffmpeg()?;

    // **Gathered before anything is written**, so a source that is not there is a refusal at the
    // door rather than an error four files into a run that has already changed the output folder.
    let mut files: Vec<PathBuf> = Vec::new();
    for source in &request.sources {
        for file in files_of(source)? {
            // Naming a folder and a file inside it is one file, not two.
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }

    // **Nothing is made on a dry run**, which is the whole of what a dry run promises. Everywhere
    // else this is the first thing, because every destination below is inside it.
    if !request.dry_run {
        std::fs::create_dir_all(&request.out)
            .with_context(|| format!("creating {}", request.out.display()))?;
    }

    // Both resolved once for the whole run rather than per file: the step is a property of the
    // request, and asking ffmpeg twenty times what it can encode is twenty subprocesses.
    let encode = request.video.unwrap_or_default().encode();
    let encoders = if request.dry_run {
        None
    } else {
        Some(profile::encoders()?)
    };

    let mut outcome = Outcome::default();
    let mut stopped = on_event(Event::Found { count: files.len() }) == Flow::Stop;

    for source in &files {
        // **Between files, which is the only place stopping can take effect.** ffmpeg is handed a
        // whole file and offers no way to be asked for half of one, so somebody who pressed Stop
        // partway through a batch of twenty means the other nineteen.
        if stopped {
            break;
        }

        let verdict = one(
            source,
            &request.out,
            &encode,
            encoders.as_ref(),
            &mut on_event,
        );
        match &verdict {
            Verdict::Converted { .. } | Verdict::WouldConvert { .. } => outcome.converted += 1,
            Verdict::Copied { .. } | Verdict::WouldCopy { .. } => outcome.copied += 1,
            _ => outcome.refused += 1,
        }
        stopped |= on_event(Event::Done {
            source: source.clone(),
            verdict,
        }) == Flow::Stop;
    }

    outcome.stopped = stopped;
    Ok(outcome)
}

/// Every file one named source comes to.
///
/// **A folder is not descended into.** What somebody points at is what they meant; a tree walk turns
/// one wrong path into hours of encoding, and a folder of songs with a folder of raw rips beside it
/// is an ordinary way to keep them.
fn files_of(source: &Path) -> Result<Vec<PathBuf>> {
    if source.is_file() {
        return Ok(vec![source.to_path_buf()]);
    }
    if !source.is_dir() {
        bail!("there is no file or folder at {}", source.display());
    }

    let listing =
        std::fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?;
    let mut files: Vec<PathBuf> = listing
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && looks_like_video(path))
        .collect();
    // So that a run over one folder reports its files in the same order twice.
    files.sort();
    Ok(files)
}

/// Where one source's result goes.
///
/// Its own name under the output folder, carrying the container's extension. The name is the file's
/// and not something rebuilt from its tags, because a file on disk has already been named by
/// whoever put it there.
#[must_use]
pub fn destination(source: &Path, out: &Path) -> PathBuf {
    let name = source.file_name().unwrap_or(source.as_os_str());
    out.join(name).with_extension(profile::DEFAULT.container)
}

/// What one file needs, and why.
///
/// **A pure function of the two questions**, so the decision a whole run turns on can be asserted
/// without an encoder and without a file. Those questions are whether packaging would take the file
/// as it is, which is [`profile::DEFAULT`]'s to say, and whether its picture is larger than this run
/// asked for.
///
/// **The size is asked only where it asks for less than the profile allows.** At the largest step the
/// two ceilings are the same number, so the profile's own finding already says it and a second line
/// would say it twice.
#[must_use]
pub fn wanted(report: &check::Report, encode: &size::Encode) -> Work {
    let mut reasons: Vec<String> = report.mismatches.iter().map(ToString::to_string).collect();

    let below_the_profile = encode.max_width < profile::DEFAULT.max_width
        || encode.max_height < profile::DEFAULT.max_height;
    if below_the_profile && let Some(over) = encode.exceeded_by(&report.info) {
        reasons.push(over);
    }

    if reasons.is_empty() {
        Work::Copy
    } else {
        Work::Encode {
            summary: reasons.join("; "),
        }
    }
}

/// Deals with one file.
///
/// `encoders` is `None` on a dry run, which is what makes every answer below a `Would` one.
fn one(
    source: &Path,
    out: &Path,
    encode: &size::Encode,
    encoders: Option<&profile::Encoders>,
    on_event: &mut impl FnMut(Event) -> Flow,
) -> Verdict {
    let destination = destination(source, out);

    // **Before the file is read**, because both of these are refusals whatever is inside it — and
    // the first is a file that would otherwise be ffmpeg's input and its output at once.
    if destination == source || destination.exists() {
        return Verdict::Exists { destination };
    }

    let report = match check::inspect(source) {
        Ok(report) => report,
        // Not fatal, and not a lie either: the file is on disk and this could not read it.
        Err(error) => {
            return Verdict::Unreadable {
                why: format!("{error:#}"),
            };
        }
    };

    let work = wanted(&report, encode);

    let Some(encoders) = encoders else {
        return match work {
            Work::Copy => Verdict::WouldCopy { destination },
            Work::Encode { summary } => Verdict::WouldConvert {
                destination,
                summary,
            },
        };
    };

    let Work::Encode { summary } = work else {
        return match std::fs::copy(source, &destination) {
            Ok(_) => Verdict::Copied { destination },
            Err(why) => Verdict::Failed {
                why: format!("copying to {}: {why}", destination.display()),
            },
        };
    };

    // Through a temporary name and a rename, as packaging itself does, so an interrupted encode
    // cannot leave a half-written video among somebody's songs.
    let partial = destination.with_extension(format!("{}.part", profile::DEFAULT.container));

    let argv = profile::transcode_argv(source, &partial, encode, &report.info, encoders);
    // **The answer is not read.** Stopping takes effect between files, so a caller saying so here is
    // heard at the next one; what this event is for is a front end showing what it ran.
    on_event(Event::Command(crate::fetch::command_line(
        std::ffi::OsStr::new(profile::FFMPEG),
        &argv,
    )));

    let named = source.to_path_buf();
    let encoded = profile::transcode(
        source,
        &partial,
        encode,
        &report.info,
        encoders,
        |progress| {
            on_event(Event::Converting {
                source: named.clone(),
                percent: progress.percent(),
            });
        },
    );

    // **A failed encode takes its own leavings with it.** What it leaves behind is a `.part` beside
    // somebody's songs, and a half-written video that shares a stem with a whole one is the kind of
    // thing found months later by a packager refusing it.
    if let Err(why) = encoded {
        let _ = std::fs::remove_file(&partial);
        return Verdict::Failed {
            why: format!("{why:#}"),
        };
    }
    if let Err(why) = std::fs::rename(&partial, &destination) {
        let _ = std::fs::remove_file(&partial);
        return Verdict::Failed {
            why: format!("renaming {} into place: {why}", partial.display()),
        };
    }

    Verdict::Converted {
        destination,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::VideoInfo;

    /// A file in exactly the shape packaging wants, which is what every question here is asked
    /// about.
    fn in_profile() -> VideoInfo {
        VideoInfo {
            duration_ms: 273_020,
            width: 1920,
            height: 1080,
            frame_rate_milli: 29_970,
            audio_sample_rate: 44_100,
            video_codec: "h264".to_owned(),
            audio_codec: "aac".to_owned(),
            pixel_format: "yuv420p".to_owned(),
            audio_channels: 2,
            title: None,
            artist: None,
        }
    }

    /// A report over a file of that shape, named as it would be on disk.
    fn report_of(info: VideoInfo, name: &str) -> check::Report {
        let path = PathBuf::from(name);
        check::Report {
            mismatches: profile::DEFAULT.check(&info, &path),
            path,
            info,
        }
    }

    /// A folder of this test's own, so two of them cannot tread on each other.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("km-video-convert").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder to work in");
        dir
    }

    /// The name is the file's own, and the extension is the container's.
    #[test]
    fn a_destination_is_the_source_name_under_the_output_folder() {
        let out = Path::new("songs");
        assert_eq!(
            destination(Path::new("/rips/A Song.mkv"), out),
            out.join("A Song.mp4")
        );
        assert_eq!(
            destination(Path::new("/rips/A Song.MKV"), out),
            out.join("A Song.mp4"),
            "the container decides the extension, not the source"
        );
    }

    /// A stem with a dot in it is ordinary in a folder of rips, and only the last part of the name
    /// is an extension.
    #[test]
    fn a_dotted_name_keeps_everything_but_its_extension() {
        assert_eq!(
            destination(Path::new("Band - Song 2.0.webm"), Path::new("out")),
            Path::new("out").join("Band - Song 2.0.mp4")
        );
    }

    /// The claim the whole run rests on: a file packaging would copy is not worth an hour of CPU
    /// and a generation of picture.
    #[test]
    fn a_file_already_in_profile_is_copied_rather_than_re_encoded() {
        let report = report_of(in_profile(), "A Song.mp4");
        assert!(report.in_profile());
        assert_eq!(wanted(&report, &size::Video::Full.encode()), Work::Copy);
    }

    /// The rule that is the opposite of a fetch's: here the re-encode is what was asked for, so a
    /// picture larger than the size named is a reason to spend the CPU.
    #[test]
    fn a_size_asked_for_is_a_reason_to_re_encode_here() {
        let report = report_of(in_profile(), "A Song.mp4");
        let Work::Encode { summary } = wanted(&report, &size::Video::Small.encode()) else {
            panic!("1080p overruns a 720p request");
        };
        assert!(summary.contains("1280x720"), "{summary}");
    }

    /// At the largest step the profile's ceiling and the size asked for are the same number, so
    /// only one of them says so.
    #[test]
    fn an_oversized_picture_is_not_reported_twice_at_the_largest_size() {
        let mut huge = in_profile();
        huge.width = 3840;
        huge.height = 2160;
        let report = report_of(huge, "A Song.mp4");

        let Work::Encode { summary } = wanted(&report, &size::Video::Full.encode()) else {
            panic!("4K is outside the profile");
        };
        assert_eq!(
            summary.matches("3840x2160").count(),
            1,
            "one finding about the picture, not two: {summary}"
        );
    }

    /// Everything the profile found reaches the summary, because that is what somebody reads when
    /// deciding whether the re-encode was worth it.
    #[test]
    fn what_is_wrong_with_a_file_is_said_in_full() {
        let mut awkward = in_profile();
        awkward.video_codec = "vp9".to_owned();
        awkward.audio_codec = "opus".to_owned();
        let report = report_of(awkward, "A Song.webm");

        let Work::Encode { summary } = wanted(&report, &size::Video::Full.encode()) else {
            panic!("VP9 and Opus in a webm is three findings");
        };
        assert!(summary.contains("VP9"), "{summary}");
        assert!(summary.contains("OPUS"), "{summary}");
        assert!(summary.contains(".webm"), "{summary}");
    }

    /// A folder gives up the videos in it and nothing else, and gives them up in one order.
    #[test]
    fn a_folder_gives_its_videos_and_leaves_everything_else_alone() {
        let dir = scratch("folder");
        for name in [
            "b.mkv",
            "a.mp4",
            "c.WEBM",
            "notes.txt",
            ".km-fetched.txt",
            "km-video-fetch.kmvf",
            "cover.jpg",
        ] {
            std::fs::write(dir.join(name), "x").expect("write");
        }
        std::fs::create_dir_all(dir.join("raw.mkv")).expect("a folder named like a video");

        let found = files_of(&dir).expect("a folder is readable");
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.mp4", "b.mkv", "c.WEBM"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Somebody who names a file has said which file they mean, whatever it is called.
    #[test]
    fn a_file_named_on_its_own_is_taken_whatever_its_extension() {
        let dir = scratch("named");
        let odd = dir.join("a song.ogv");
        std::fs::write(&odd, "x").expect("write");

        assert_eq!(files_of(&odd).expect("a file is a file"), vec![odd.clone()]);
        assert!(
            files_of(&dir).expect("readable").is_empty(),
            "and the same file is not picked up by naming the folder"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A path that is neither is a refusal at the door, before anything has been written.
    #[test]
    fn a_source_that_is_not_there_is_refused_before_the_run_starts() {
        let missing = scratch("missing").join("nowhere.mp4");
        let refused = files_of(&missing).expect_err("there is nothing there");
        assert!(refused.to_string().contains("nowhere.mp4"), "{refused}");
    }

    /// The output folder is somebody's library, and a run must not overwrite what is in it — nor
    /// read and write one file at once, which is what a `.mp4` source in the output folder is.
    #[test]
    fn a_name_already_taken_is_refused_rather_than_overwritten() {
        let dir = scratch("taken");
        let out = dir.join("out");
        std::fs::create_dir_all(&out).expect("an output folder");

        let source = dir.join("A Song.mkv");
        std::fs::write(&source, "x").expect("write");
        std::fs::write(out.join("A Song.mp4"), "already here").expect("write");

        let mut said = Vec::new();
        let verdict = one(
            &source,
            &out,
            &size::Video::Full.encode(),
            None,
            &mut |event| {
                said.push(event);
                Flow::Go
            },
        );
        assert_eq!(
            verdict,
            Verdict::Exists {
                destination: out.join("A Song.mp4")
            }
        );
        assert!(said.is_empty(), "nothing was read and nothing was run");
        assert_eq!(
            std::fs::read_to_string(out.join("A Song.mp4")).unwrap(),
            "already here"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same refusal, reached the other way: an `.mp4` that is already in the output folder is
    /// its own destination.
    #[test]
    fn a_file_that_is_its_own_destination_is_refused() {
        let dir = scratch("itself");
        let source = dir.join("A Song.mp4");
        std::fs::write(&source, "x").expect("write");

        let verdict = one(
            &source,
            &dir,
            &size::Video::Full.encode(),
            None,
            &mut |_| Flow::Go,
        );
        assert_eq!(
            verdict,
            Verdict::Exists {
                destination: source.clone()
            }
        );
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            "x",
            "and it is still there"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Naming a folder and a file inside it is one file, not two — and two names for one file is
    /// one encode rather than an encode and then a refusal.
    #[test]
    fn a_file_named_twice_is_converted_once() {
        let dir = scratch("twice");
        let source = dir.join("a.mkv");
        std::fs::write(&source, "x").expect("write");

        let request = Request {
            sources: vec![dir.clone(), source.clone()],
            out: dir.join("out"),
            video: None,
            dry_run: true,
        };

        let mut found = None;
        // Every file here is unreadable — they hold one byte — which is exactly what makes this
        // assertable without ffmpeg: what is being counted is how many were reached.
        let outcome = convert(&request, |event| {
            if let Event::Found { count } = event {
                found = Some(count);
            }
            Flow::Go
        });
        // An `Err` is a machine with no ffmpeg on it, which is not this test's subject.
        if let Ok(outcome) = outcome {
            assert_eq!(found, Some(1), "one file, named two ways");
            assert_eq!(outcome.refused, 1);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing at all is a refusal with a sentence rather than a run that quietly does nothing.
    #[test]
    fn nothing_named_is_refused_with_a_sentence() {
        let request = Request {
            sources: Vec::new(),
            out: PathBuf::from("."),
            video: None,
            dry_run: true,
        };
        let refused = convert(&request, |_| Flow::Go).expect_err("nothing to do");
        assert!(
            refused.to_string().contains("name a video file"),
            "{refused}"
        );
    }

    /// The extension list is what a folder is searched by, and it is matched however it is spelled.
    #[test]
    fn a_video_is_recognised_whatever_case_its_extension_is_in() {
        for name in ["a.mp4", "a.MKV", "a.WebM", "a.mov", "a.avi", "a.m4v"] {
            assert!(looks_like_video(Path::new(name)), "{name}");
        }
        for name in ["a.txt", "a.kmvf", "a.jpg", "a.mp3", "a"] {
            assert!(!looks_like_video(Path::new(name)), "{name}");
        }
    }
}
