//! Fetch video songs with yt-dlp, in the shape packaging wants them.
//!
//! # Why this exists rather than a shell alias
//!
//! Three things, none of which a remembered command line does well.
//!
//! 1. **It asks for the shape the machine wants.** [`profile::DEFAULT`] is H.264 in 8-bit 4:2:0, at
//!    most 1080p30, with AAC, in MP4. A download asked for AVC and AAC arrives as exactly that, and
//!    packaging copies its bytes; a download that took whatever was offered arrives as VP9 at 60 fps
//!    and costs an hour of re-encoding per song. The selector that gets this right is not something
//!    anybody should be retyping.
//! 2. **It writes down what the song is.** A video's title and artist have nowhere to live except
//!    the container's tags and the file's name, and yt-dlp knows both at the moment it downloads.
//!    Written then, they survive into curation; not written then, they are gone and somebody types
//!    them in by hand later.
//! 3. **It says whether the first thing worked.** It probes what arrived and measures it against the
//!    same profile packaging will, so a song that will need re-encoding is known now rather than
//!    during a package build.
//!
//! # Where the line is drawn
//!
//! This is a **packager's** tool and it is deliberately not part of the karaoke app — which is
//! the whole reason it lives in a repository of its own. Nothing in that product reaches the network
//! for a song, at run time or at any other time; the appliance may have no internet at all. What
//! this program does is what a person asked it to do, one URL at a time.
//!
//! # This file is the command line and nothing else
//!
//! Every decision worth reusing is in `km-video-core`; what is left here is argument parsing and
//! what gets printed. That boundary is what lets a second program in this repository — the web UI
//! the workspace is laid out for — fetch and check exactly the same way without inheriting a CLI.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;
use km_video_core::{args, convert, fetch, list, run, size};

/// Fetch video songs with yt-dlp, in the shape packaging wants them.
#[derive(Debug, Parser)]
#[command(name = "km-video-fetch", version, about, long_about = None)]
struct Cli {
    /// Video or playlist URLs to fetch.
    #[arg(value_name = "URL")]
    targets: Vec<String>,

    /// Convert a video already on disk into the shape packaging wants.
    ///
    /// Give a video file, or a folder to take every video in it. The result goes into --out under
    /// the same name with a .mp4 extension, and the file you named is left where it is. A video
    /// already in the right shape is copied rather than re-encoded.
    ///
    /// Repeat this to name more than one. --video and --dry-run apply; the options that belong to
    /// a download do not.
    // The refusal is clap's rather than a `bail!` here, so an argv that cannot mean anything is
    // turned down before the program starts, worded by the parser and validated by `debug_assert`
    // in the test below.
    #[arg(long, value_name = "PATH", conflicts_with_all = [
        "targets", "from_file", "playlist", "limit", "no_archive", "cookies_from_browser",
        "subs", "format", "sort", "normalize", "yt_dlp",
    ])]
    convert: Vec<PathBuf>,

    /// Read URLs from a file, one per line.
    ///
    /// With no URLs and no file named, a `km-video-fetch.kmvf` in the destination folder is read as
    /// this, so a folder can carry its own list.
    ///
    /// A line may start with `--playlist`, `--no-playlist` or `--out FOLDER` to say what that one
    /// link is and where it goes, `FOLDER` being under `--out`. A list that says none of those is
    /// handed to yt-dlp exactly as it is.
    ///
    /// Lines before the first link are a header, and may carry `--playlist`, `--subs`,
    /// `--normalize`, `--no-archive`, `--limit N`, `--cookies-from-browser BROWSER`,
    /// `--format SELECTOR`, `--sort ORDER` and `--video SIZE` for the whole list. What is given
    /// here wins.
    #[arg(long, value_name = "PATH")]
    from_file: Option<PathBuf>,

    /// Expand a playlist instead of taking the single video from its URL.
    ///
    /// Also the answer for a line in a `--from-file` list that does not say for itself.
    #[arg(long)]
    playlist: bool,

    /// Where the files go.
    #[arg(short, long, value_name = "DIR", default_value = ".")]
    out: PathBuf,

    /// Take at most this many items from a playlist.
    #[arg(long, value_name = "N")]
    limit: Option<u32>,

    /// Fetch videos that are already in the archive again.
    #[arg(long)]
    no_archive: bool,

    /// Take cookies from a browser, for material that needs an account.
    ///
    /// One of brave, chrome, chromium, edge, firefox, opera, safari, vivaldi or whale, optionally
    /// with a profile after a colon — `firefox:work`. Close that browser first: Chrome and Edge
    /// keep their cookie database locked while they are running.
    #[arg(long, value_name = "BROWSER")]
    cookies_from_browser: Option<String>,

    /// Mux subtitles into the file. Off by default; the machine does not read them.
    #[arg(long)]
    subs: bool,

    /// Replace the format selector.
    #[arg(long, value_name = "SELECTOR")]
    format: Option<String>,

    /// Replace the format sort order.
    #[arg(long, value_name = "ORDER")]
    sort: Option<String>,

    /// How much picture to fetch: full (1080p), small (720p) or tiny (480p).
    ///
    /// The sound is fetched and stored at full quality whichever you choose, so a smaller size
    /// costs nothing you can hear. With --normalize, anything that lands larger than this is
    /// re-encoded down to it.
    ///
    /// --format replaces the whole selector, so where both are given --format decides what is
    /// downloaded and this decides what --normalize re-encodes to.
    #[arg(long, value_name = "SIZE")]
    video: Option<size::Video>,

    /// Re-encode anything that landed outside the packaging profile.
    #[arg(long)]
    normalize: bool,

    /// Fail if any file cannot be played as it is, or with --convert did not come through.
    #[arg(long)]
    strict: bool,

    /// Say what would be fetched and fetch nothing.
    #[arg(long)]
    dry_run: bool,

    /// Print the yt-dlp command line before running it.
    #[arg(long)]
    show_command: bool,

    /// The yt-dlp to run, when it is not on the PATH.
    #[arg(long, value_name = "PATH")]
    yt_dlp: Option<PathBuf>,
}

/// Exit 2 means a file arrived that the machine cannot play, under `--strict`.
///
/// A code of its own rather than a plain failure: a finding and a broken run are different things,
/// and a script that treats them the same cannot tell "this download needs attention" from "yt-dlp
/// is not installed".
const UNPLAYABLE: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(UNPLAYABLE),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the fetch, returning whether everything is acceptable.
fn run() -> Result<bool> {
    let cli = Cli::parse();

    if !cli.convert.is_empty() {
        return converting(&cli);
    }

    if !cli.targets.is_empty() && cli.from_file.is_some() {
        bail!("give URLs or --from-file, not both");
    }

    // **Resolved here rather than left to `fetch`**, which would do the same thing and say so.
    // The reason is the list's header: it may set what the flags below set, so it has to be read
    // before the request is built, and reading it means knowing which file it is. `fetch` keeps
    // its fallback for the case this leaves — nothing named and no list anywhere — because the
    // message it refuses with names the file somebody should make.
    //
    // An explicit `--from-file` wins, and URLs on the command line mean the folder's own list is
    // not consulted at all.
    let from_file = cli.from_file.clone().or_else(|| {
        cli.targets
            .is_empty()
            .then(|| args::Plan::folders_own_list(&cli.out))
            .flatten()
    });
    if cli.from_file.is_none()
        && let Some(list) = &from_file
    {
        // `fetch` would have said this through an event; it is this program's sentence either way.
        eprintln!("reading {}", list.display());
    }

    let mut request = fetch::Request {
        plan: args::Plan {
            targets: cli.targets.clone(),
            from_file: from_file.clone(),
            playlist: cli.playlist,
            out: cli.out.clone(),
            limit: cli.limit,
            archive: (!cli.no_archive).then(|| args::Plan::default_archive(&cli.out)),
            cookies_from_browser: cli.cookies_from_browser.clone(),
            subs: cli.subs,
            video: cli.video,
            format: cli.format.clone(),
            sort: cli.sort.clone(),
            dry_run: cli.dry_run,
            // Set by the request's `progress`; stated here only because the struct is exhaustive.
            progress_lines: false,
        },
        yt_dlp: cli.yt_dlp.clone(),
        normalize: cli.normalize,
        // **The terminal is handed to yt-dlp**, which draws a better bar than anything this could
        // rebuild from a pipe. The web UI is the caller that cannot do that.
        progress: fetch::Progress::Terminal,
    };

    // **After the request, and by the request's own rule**: what was asked for on the command line
    // wins where it can be told to have been asked for, and a flag is the *or* of the two. See
    // `fetch::Request::apply_list_settings`, which is where that is written down and where the one
    // wart in it is admitted.
    if let Some(list) = &from_file {
        request.apply_list_settings(&list::settings_of(list));
    }

    let mut reporter = Reporter {
        show_command: cli.show_command || cli.dry_run,
        dry_run: cli.dry_run,
        normalizing: u8::MAX,
    };
    // The command line never stops of its own accord: the child owns the terminal, and Ctrl-C
    // reaches yt-dlp directly.
    let outcome = fetch::fetch(&request, |event| {
        reporter.say(&event);
        fetch::Flow::Go
    })?;

    if !outcome.all_playable && cli.strict {
        eprintln!("\nat least one file cannot be played as it is; --normalize re-encodes them");
    }
    Ok(outcome.all_playable || !cli.strict)
}

/// Runs the conversion, returning whether every file came through.
///
/// The other half of [`run`], reached by `--convert`. What the two share is everything below
/// `km-video-core`: one profile, one set of sizes, one re-encode.
fn converting(cli: &Cli) -> Result<bool> {
    let request = convert::Request {
        sources: cli.convert.clone(),
        out: cli.out.clone(),
        video: cli.video,
        dry_run: cli.dry_run,
    };

    let mut reporter = Reporter {
        show_command: cli.show_command || cli.dry_run,
        dry_run: cli.dry_run,
        normalizing: u8::MAX,
    };
    // Nothing stops of its own accord here: ffmpeg is a child of this process, so Ctrl-C reaches it
    // directly, exactly as it reaches yt-dlp during a fetch.
    let outcome = convert::convert(&request, |event| {
        reporter.converting(&event);
        convert::Flow::Go
    })?;

    println!("\n{}", converted(&outcome, cli.dry_run));
    if outcome.refused > 0 && cli.strict {
        eprintln!("\nat least one file produced nothing");
    }
    Ok(outcome.refused == 0 || !cli.strict)
}

/// How a finished conversion is described in one line.
fn converted(outcome: &convert::Outcome, dry_run: bool) -> String {
    if outcome.converted == 0 && outcome.copied == 0 && outcome.refused == 0 {
        return "no videos to convert".to_owned();
    }

    let mut parts = Vec::new();
    if outcome.converted > 0 {
        let verb = if dry_run {
            "would convert"
        } else {
            "converted"
        };
        parts.push(format!(
            "{verb} {}",
            plural(outcome.converted, "video", "videos")
        ));
    }
    if outcome.copied > 0 {
        let verb = if dry_run { "would copy" } else { "copied" };
        parts.push(format!(
            "{verb} {} already in profile",
            plural(outcome.copied, "video", "videos")
        ));
    }
    if outcome.refused > 0 {
        parts.push(format!(
            "{} produced nothing",
            plural(outcome.refused, "video", "videos")
        ));
    }

    let said = parts.join(", ");
    // **Said first, because it changes what every other number means.** A run somebody stopped did
    // not convert nothing; it converted what it had reached.
    if outcome.stopped {
        return format!("stopped — {said} first");
    }
    said
}

/// Turns what happened into what is printed, and holds the little state that needs.
///
/// **Every sentence the tool says is here.** `km-video-core` decides nothing about wording, and the
/// web UI renders the same events into HTML without either of them knowing about the other.
struct Reporter {
    /// Whether the yt-dlp command line is worth showing.
    show_command: bool,
    /// A dry run reports what *would* arrive, in a different shape and to stdout.
    dry_run: bool,
    /// The last re-encode percentage drawn, so the in-place line is only redrawn when it moves.
    /// `u8::MAX` means none has been drawn and there is nothing to erase.
    normalizing: u8,
}

impl Reporter {
    fn say(&mut self, event: &fetch::Event) {
        use fetch::Event as E;
        match event {
            E::ReadingList(list) => eprintln!("reading {}", list.display()),

            E::Tool {
                version,
                stale_days,
            } => {
                eprintln!("yt-dlp {version}");
                if let Some(age) = stale_days
                    && *age > run::STALE_AFTER_DAYS
                {
                    eprintln!(
                        "  warning: that is {age} days old. YouTube changes what it serves and \
                         yt-dlp follows; a stale copy fails in ways that look like a broken \
                         network."
                    );
                }
            }

            E::Command(line) => {
                if self.show_command {
                    eprintln!("\n{line}");
                }
                // The blank line that separates this program's preamble from yt-dlp's own output,
                // which starts the moment this event has been handled.
                eprintln!();
            }

            // Only ever emitted under `Progress::Watched`, which this front end does not ask for:
            // yt-dlp has the terminal and is drawing on it already.
            E::Said(_) | E::Downloading { .. } => {}

            E::Downloaded { ok, fetched } => {
                if !ok {
                    eprintln!("\nyt-dlp reported a failure; what did arrive is below.");
                }
                match (*fetched, self.dry_run) {
                    (0, true) => println!("nothing to fetch"),
                    (0, false) => println!(
                        "nothing new — every url was already in the archive, or none produced a \
                         file"
                    ),
                    (count, true) => println!("would fetch {}:", plural(count, "video", "videos")),
                    (count, false) => {
                        println!("\nfetched {}:", plural(count, "video", "videos"));
                    }
                }
            }

            // Drawn in place and erased when it finishes, so a long re-encode says something
            // without leaving a hundred lines behind. Only where there is a terminal to draw on:
            // redirected to a file this would be a hundred lines behind.
            E::Normalizing { percent, .. } => {
                use std::io::IsTerminal;
                if std::io::stderr().is_terminal()
                    && *percent != self.normalizing
                    && percent % 10 == 0
                {
                    self.normalizing = *percent;
                    eprint!("      re-encoding {percent}%\r");
                }
            }

            E::Arrived { record, verdict } => {
                if self.normalizing != u8::MAX {
                    eprint!("\r{:width$}\r", "", width = 28);
                    self.normalizing = u8::MAX;
                }
                if self.dry_run {
                    match record.length() {
                        Some(length) => println!("  {}  ({length})", record.describe()),
                        None => println!("  {}", record.describe()),
                    }
                    return;
                }
                match arrival(record, verdict) {
                    (name, Some(line)) => println!("  {name}\n      {line}"),
                    (name, None) => println!("  {name}"),
                }
            }
        }
    }

    /// The same job for a conversion: one narration, rendered as lines.
    fn converting(&mut self, event: &convert::Event) {
        use convert::Event as E;
        match event {
            E::Found { count } => {
                let verb = if self.dry_run {
                    "would look at"
                } else {
                    "converting"
                };
                println!("{verb} {}:", plural(*count, "video", "videos"));
            }

            E::Command(line) => {
                if self.show_command {
                    eprintln!("  {line}");
                }
            }

            // Drawn in place and erased when the file is done, so a re-encode that takes minutes
            // says something without leaving a hundred lines behind. Only where there is a terminal
            // to draw on: redirected to a file this would be a hundred lines.
            E::Converting { percent, .. } => {
                use std::io::IsTerminal;
                if std::io::stderr().is_terminal()
                    && *percent != self.normalizing
                    && percent % 10 == 0
                {
                    self.normalizing = *percent;
                    eprint!("      converting {percent}%\r");
                }
            }

            E::Done { source, verdict } => {
                if self.normalizing != u8::MAX {
                    eprint!("\r{:width$}\r", "", width = 28);
                    self.normalizing = u8::MAX;
                }
                println!("  {}\n      {}", name_of(source), became(verdict));
            }
        }
    }
}

/// A path's last component, or the whole thing where it has none.
fn name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// What became of one converted file, in one sentence.
fn became(verdict: &convert::Verdict) -> String {
    use convert::Verdict as V;
    match verdict {
        V::Copied { destination } => {
            format!("already in profile — copied to {}", name_of(destination))
        }
        V::Converted {
            destination,
            summary,
        } => format!("converted to {} ({summary})", name_of(destination)),
        V::WouldCopy { .. } => "already in profile — would be copied as it is".to_owned(),
        V::WouldConvert { summary, .. } => format!("would be converted: {summary}"),
        V::Exists { destination } => {
            format!("not converted — {} is there already", destination.display())
        }
        V::Unreadable { why } => format!("could not be read: {why}"),
        V::Failed { why } => format!("failed: {why}"),
    }
}

/// One arrived file as a name and, where there is something to say, a line about it.
fn arrival(record: &run::Record, verdict: &fetch::Verdict) -> (String, Option<String>) {
    use fetch::Verdict as V;

    let Some(path) = record.path() else {
        return (format!("{} — no file was written", record.describe()), None);
    };
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );

    let line = match verdict {
        V::InProfile => Some("in profile — packaging will copy it".to_owned()),
        V::Outside { summary } => Some(format!(
            "outside the profile: {summary} — packaging will re-encode it"
        )),
        V::Unplayable { summary } => Some(format!("cannot be played as it is: {summary}")),
        V::Normalized { summary } => Some(format!("re-encoded into profile ({summary})")),
        V::Larger { summary } => Some(format!(
            "in profile, and larger than the size asked for: {summary}"
        )),
        V::Shrunk { summary } => Some(format!("re-encoded smaller ({summary})")),
        V::Unreadable { why } => Some(format!("could not be probed: {why}")),
        V::NotFetched => None,
    };
    (name, line)
}

/// `1 video` / `2 videos`.
fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// Every flag the documentation mentions, asserted to exist, so a flag cannot live in the
    /// README and nowhere else.
    #[test]
    fn every_documented_flag_parses() {
        let cli = Cli::try_parse_from([
            "km-video-fetch",
            "--playlist",
            "--out",
            "videos",
            "--limit",
            "10",
            "--no-archive",
            "--cookies-from-browser",
            "firefox",
            "--subs",
            "--format",
            "bestvideo+bestaudio",
            "--sort",
            "res",
            "--video",
            "small",
            "--normalize",
            "--strict",
            "--dry-run",
            "--show-command",
            "--yt-dlp",
            "/opt/bin/yt-dlp",
            "https://example.invalid/a",
        ])
        .expect("every documented flag exists");

        assert!(cli.playlist);
        assert_eq!(cli.out, PathBuf::from("videos"));
        assert_eq!(cli.limit, Some(10));
        assert!(cli.no_archive);
        assert_eq!(cli.cookies_from_browser.as_deref(), Some("firefox"));
        assert!(cli.subs);
        assert_eq!(cli.video, Some(size::Video::Small));
        assert!(cli.normalize);
        assert!(cli.strict);
        assert!(cli.dry_run);
        assert!(cli.show_command);
        assert_eq!(cli.targets, vec!["https://example.invalid/a".to_owned()]);
    }

    /// A size nobody named stays unsaid, so that a list saying one is heard. The flag carrying a
    /// default instead would overrule every list in the tree with a word nobody typed.
    #[test]
    fn a_size_nobody_asked_for_is_left_unsaid() {
        let cli = Cli::try_parse_from(["km-video-fetch", "https://example.invalid/a"])
            .expect("a URL is enough");
        assert_eq!(cli.video, None);
    }

    /// The three words reach a person from whichever surface refused them.
    #[test]
    fn an_unknown_size_is_refused_by_name() {
        let refused = Cli::try_parse_from(["km-video-fetch", "--video", "huge", "x"])
            .expect_err("huge is not a size");
        let said = refused.to_string();
        assert!(said.contains("huge"), "{said}");
        for word in size::Video::WORDS {
            assert!(said.contains(word), "{said}");
        }
    }

    #[test]
    fn urls_can_come_from_a_file_instead() {
        let cli = Cli::try_parse_from(["km-video-fetch", "--from-file", "songs.txt"])
            .expect("a batch file is a way of naming targets");
        assert_eq!(cli.from_file, Some(PathBuf::from("songs.txt")));
        assert!(cli.targets.is_empty());
    }

    #[test]
    fn the_destination_defaults_to_here() {
        let cli = Cli::try_parse_from(["km-video-fetch", "https://example.invalid/a"]).unwrap();
        assert_eq!(cli.out, PathBuf::from("."));
        assert!(!cli.no_archive, "the archive is on unless turned off");
    }

    /// A folder carrying its own list is read when nothing else was named, and only then.
    ///
    /// **The same argument the archive beside it already makes**: what to fetch *into this folder*
    /// is a fact about the folder, and one somebody maintains by hand over months. The negative
    /// halves matter as much: a folder with no such file is an ordinary first run rather than a
    /// fault, and a folder that does not exist yet must not be an error either.
    #[test]
    fn a_folder_can_carry_its_own_list() {
        let dir = std::env::temp_dir().join(format!(
            "km-video-fetch-own-list-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");

        // Nothing there yet.
        assert_eq!(args::Plan::folders_own_list(&dir), None);
        // Nor is a folder that does not exist at all.
        assert_eq!(args::Plan::folders_own_list(&dir.join("absent")), None);

        let list = dir.join(args::BATCH_NAME);
        std::fs::write(&list, "https://example.invalid/a\n").expect("write the list");
        assert_eq!(args::Plan::folders_own_list(&dir), Some(list));

        // A directory of that name is not a list, which is the one shape `is_file` is here for.
        let other = dir.join("other");
        std::fs::create_dir_all(other.join(args::BATCH_NAME)).expect("a folder of that name");
        assert_eq!(args::Plan::folders_own_list(&other), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of the command line, and the refusal that keeps the two apart: a run is a
    /// fetch or a conversion, and an argv asking for both cannot mean anything.
    #[test]
    fn a_conversion_names_files_and_refuses_the_arguments_of_a_download() {
        let cli = Cli::try_parse_from([
            "km-video-fetch",
            "--convert",
            "rips/A Song.mkv",
            "--convert",
            "rips",
            "--out",
            "songs",
            "--video",
            "small",
            "--dry-run",
        ])
        .expect("a conversion is a command line of its own");
        assert_eq!(
            cli.convert,
            vec![PathBuf::from("rips/A Song.mkv"), PathBuf::from("rips")],
            "repeated, and in the order they were named"
        );
        assert_eq!(cli.out, PathBuf::from("songs"));
        assert_eq!(cli.video, Some(size::Video::Small));
        assert!(cli.dry_run);

        assert!(
            Cli::try_parse_from([
                "km-video-fetch",
                "--convert",
                "a.mkv",
                "https://example.invalid/a"
            ])
            .is_err(),
            "a URL is for yt-dlp and a file is not"
        );
        assert!(
            Cli::try_parse_from(["km-video-fetch", "--convert", "a.mkv", "--playlist"]).is_err(),
            "there is no playlist on a disk"
        );
        assert!(
            Cli::try_parse_from(["km-video-fetch", "--convert", "a.mkv", "--normalize"]).is_err(),
            "converting is what was asked for already"
        );
    }

    /// The one line a conversion ends with, in each of the shapes it takes.
    #[test]
    fn a_conversion_is_described_in_one_line() {
        let nothing = convert::Outcome::default();
        assert_eq!(converted(&nothing, false), "no videos to convert");

        let mixed = convert::Outcome {
            converted: 2,
            copied: 1,
            refused: 3,
            stopped: false,
        };
        assert_eq!(
            converted(&mixed, false),
            "converted 2 videos, copied 1 video already in profile, 3 videos produced nothing"
        );
        assert_eq!(
            converted(&mixed, true),
            "would convert 2 videos, would copy 1 video already in profile, 3 videos produced nothing"
        );

        let stopped = convert::Outcome {
            converted: 1,
            stopped: true,
            ..convert::Outcome::default()
        };
        assert_eq!(
            converted(&stopped, false),
            "stopped — converted 1 video first"
        );
    }

    #[test]
    fn counts_read_as_english() {
        assert_eq!(plural(1, "video", "videos"), "1 video");
        assert_eq!(plural(0, "video", "videos"), "0 videos");
        assert_eq!(plural(4, "video", "videos"), "4 videos");
    }
}
