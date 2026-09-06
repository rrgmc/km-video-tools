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
//! This is a **packager's** tool and it is deliberately not part of the karaoke machine — which is
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
use km_video_core::{args, check, profile, run};

/// Fetch video songs with yt-dlp, in the shape packaging wants them.
#[derive(Debug, Parser)]
#[command(name = "km-video-fetch", version, about, long_about = None)]
struct Cli {
    /// Video or playlist URLs to fetch.
    #[arg(value_name = "URL")]
    targets: Vec<String>,

    /// Read URLs from a file, one per line.
    ///
    /// With no URLs and no file named, a `km-video-fetch.txt` in the destination folder is read as
    /// this — so a folder can carry its own list, the way it already carries its own archive.
    #[arg(long, value_name = "PATH")]
    from_file: Option<PathBuf>,

    /// Expand a playlist instead of taking the single video from its URL.
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

    /// Re-encode anything that landed outside the packaging profile.
    #[arg(long)]
    normalize: bool,

    /// Fail if any file cannot be played as it is.
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
/// A code of its own for the reason `km-wallpaper-pack` gives one to a failed contrast gate: a finding
/// and a broken run are different things, and a script that treats them the same cannot tell "this
/// download needs attention" from "yt-dlp is not installed".
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
    let mut cli = Cli::parse();

    if !cli.targets.is_empty() && cli.from_file.is_some() {
        bail!("give URLs or --from-file, not both");
    }
    // **A folder is allowed to say what goes in it.** With nothing named on the command line, a
    // `km-video-fetch.txt` in the destination is taken as the list — the same argument the archive
    // beside it already makes: what to fetch *into this folder* is a fact about the folder, and one
    // somebody maintains by hand over months rather than retypes.
    //
    // Said out loud, because a run that fetched forty videos nobody named must be able to say where
    // the forty came from.
    //
    // An explicit `--from-file` wins, and URLs on the command line mean the file is not consulted at
    // all — that is not the refusal above, which is about being told two things at once. Here
    // somebody has said what they want and the folder's standing list is simply not what they asked
    // for.
    if cli.targets.is_empty() && cli.from_file.is_none() {
        cli.from_file = args::Plan::folders_own_list(&cli.out);
        match &cli.from_file {
            Some(list) => eprintln!("reading {}", list.display()),
            None => bail!(
                "nothing to fetch — give a URL, or --from-file with a list of them, or put one in \
                 {}",
                cli.out.join(args::BATCH_NAME).display()
            ),
        }
    }

    let binary = run::binary(cli.yt_dlp.as_deref());
    let version = run::version(&binary)?;
    eprintln!("yt-dlp {version}");
    if let Some(age) = run::age_in_days(&version, run::today())
        && age > run::STALE_AFTER_DAYS
    {
        eprintln!(
            "  warning: that is {age} days old. YouTube changes what it serves and yt-dlp follows; \
             a stale copy fails in ways that look like a broken network."
        );
    }
    run::ensure_ffmpeg()?;

    // Made now rather than left to yt-dlp, because the archive and the record file both live in it
    // and both are opened before the first download finishes.
    std::fs::create_dir_all(&cli.out)?;

    // yt-dlp is given the bare name and resolves it against `-P`; this is the same file, spelled so
    // that it can be read back. Removed first, because a stale one from an interrupted run would
    // otherwise be reported as this run's haul.
    let records = args::Plan::records_path(&cli.out);
    let _ = std::fs::remove_file(&records);

    let plan = args::Plan {
        targets: cli.targets.clone(),
        from_file: cli.from_file.clone(),
        playlist: cli.playlist,
        out: cli.out.clone(),
        limit: cli.limit,
        archive: (!cli.no_archive).then(|| args::Plan::default_archive(&cli.out)),
        cookies_from_browser: cli.cookies_from_browser.clone(),
        subs: cli.subs,
        format: cli.format.clone(),
        sort: cli.sort.clone(),
        dry_run: cli.dry_run,
    };

    let argv = args::argv(&plan);
    if cli.show_command || cli.dry_run {
        eprintln!("\n{}", command_line(&binary, &argv));
    }

    eprintln!();
    let completed = run::spawn(&binary, &argv)?;
    let fetched = run::read_records(&records)?;
    let _ = std::fs::remove_file(&records);

    if !completed {
        eprintln!("\nyt-dlp reported a failure; what did arrive is below.");
    }

    if cli.dry_run {
        report_dry_run(&fetched);
        return Ok(true);
    }

    let acceptable = report(&fetched, cli.normalize, cli.strict)?;
    Ok(acceptable || !cli.strict)
}

/// The command line, quoted enough to be pasted back into a shell.
fn command_line(binary: &std::ffi::OsStr, argv: &[std::ffi::OsString]) -> String {
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

/// What a dry run found.
fn report_dry_run(fetched: &[run::Record]) {
    if fetched.is_empty() {
        println!("nothing to fetch");
        return;
    }
    println!("would fetch {}:", plural(fetched.len(), "video", "videos"));
    for record in fetched {
        match record.length() {
            Some(length) => println!("  {}  ({length})", record.describe()),
            None => println!("  {}", record.describe()),
        }
    }
}

/// Checks and reports what arrived, returning whether all of it is playable.
fn report(fetched: &[run::Record], normalize: bool, strict: bool) -> Result<bool> {
    if fetched.is_empty() {
        println!("nothing new — every url was already in the archive, or none produced a file");
        return Ok(true);
    }

    println!("\nfetched {}:", plural(fetched.len(), "video", "videos"));
    let mut all_playable = true;

    for record in fetched {
        let Some(path) = record.path() else {
            println!("  {} — no file was written", record.describe());
            continue;
        };
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );

        match inspect(&path, normalize) {
            Ok(Some(line)) => println!("  {name}\n      {line}"),
            Ok(None) => println!("  {name}"),
            Err(Unplayable(line)) => {
                all_playable = false;
                println!("  {name}\n      {line}");
            }
        }
    }

    if !all_playable && strict {
        eprintln!("\nat least one file cannot be played as it is; --normalize re-encodes them");
    }
    Ok(all_playable)
}

/// A file the machine could not play.
struct Unplayable(String);

/// Describes one downloaded file's shape, re-encoding it first when asked.
fn inspect(path: &std::path::Path, normalize: bool) -> Result<Option<String>, Unplayable> {
    let report = match check::inspect(path) {
        Ok(report) => report,
        // Not fatal, and not a lie either: the file is on disk and this could not read it. Said as
        // a finding so the run goes on to the next one.
        Err(error) => return Err(Unplayable(format!("could not be probed: {error:#}"))),
    };

    if report.in_profile() {
        return Ok(Some("in profile — packaging will copy it".to_owned()));
    }

    let reasons: Vec<_> = report.mismatches.iter().map(ToString::to_string).collect();
    let summary = reasons.join("; ");

    if !normalize {
        let line = if report.blocking() {
            format!("cannot be played as it is: {summary}")
        } else {
            format!("outside the profile: {summary} — packaging will re-encode it")
        };
        return if report.blocking() {
            Err(Unplayable(line))
        } else {
            Ok(Some(line))
        };
    }

    match normalize_now(&report) {
        Ok(()) => Ok(Some(format!("re-encoded into profile ({summary})"))),
        Err(error) => Err(Unplayable(format!("re-encoding failed: {error:#}"))),
    }
}

/// Re-encodes one file, drawing a progress line when there is a terminal to draw it on.
fn normalize_now(report: &check::Report) -> Result<()> {
    use std::io::IsTerminal;

    let encoders = profile::encoders()?;
    let interactive = std::io::stderr().is_terminal();
    let mut last = u8::MAX;

    check::normalize(report, &encoders, |progress| {
        let percent = progress.percent();
        if interactive && percent != last && percent % 10 == 0 {
            last = percent;
            eprint!("      re-encoding {percent}%\r");
        }
    })?;

    if last != u8::MAX {
        eprint!("\r{:width$}\r", "", width = 28);
    }
    Ok(())
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
        assert!(cli.normalize);
        assert!(cli.strict);
        assert!(cli.dry_run);
        assert!(cli.show_command);
        assert_eq!(cli.targets, vec!["https://example.invalid/a".to_owned()]);
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

    #[test]
    fn counts_read_as_english() {
        assert_eq!(plural(1, "video", "videos"), "1 video");
        assert_eq!(plural(0, "video", "videos"), "0 videos");
        assert_eq!(plural(4, "video", "videos"), "4 videos");
    }
}
