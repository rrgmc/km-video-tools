//! The yt-dlp command line, and why it is the one it is.
//!
//! This module builds an argv and spawns nothing, deliberately splitting choosing the arguments
//! from running them: the interesting part of this tool is *which* arguments it passes, and that
//! can only be asserted in a test if choosing them is separable from running them.
//!
//! # What the arguments are for
//!
//! One thing, mostly: land a file that [`crate::profile::DEFAULT`] already accepts, so packaging
//! copies its bytes instead of spending an hour re-encoding a picture it can only make worse. That
//! profile is H.264 in 8-bit 4:2:0, at most 1080p30, with AAC, in MP4 — and a YouTube download asked
//! for AVC and AAC is exactly that, so the probe-first design in [`crate::profile`] has something to
//! probe. Asking for it up front is free; discovering afterwards that VP9 arrived is not.
//!
//! # Three things deliberately *not* passed
//!
//! * **`--embed-thumbnail`.** In MP4 yt-dlp attaches cover art as a second video stream carrying
//!   `attached_pic`. A reader that takes the first video stream it finds then describes the JPEG
//!   rather than the picture. [`crate::probe`] skips such a stream, and this refuses to create the
//!   situation in the first place — two guards, because a file fetched by other means can still
//!   arrive carrying one. It buys nothing here, since the machine never shows cover art.
//! * **`--embed-subs`.** It muxes a `mov_text` stream, and the `Searching a video's words` decision
//!   in the karaoke app is that the project does not index a video's captions. Available behind
//!   `--subs` for anyone who later wants them; off is the default because it changes the shape of
//!   the file for no benefit the machine can currently use.
//! * **`--restrict-filenames`.** It strips names to ASCII, and the material this exists for is
//!   Japanese and Korean. A stem is the title of last resort, and mangling it is worse than a long
//!   one. `--windows-filenames` handles the characters that actually break a path.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The formats to consider: anything, provided the picture is no taller than 1080.
///
/// A ceiling rather than a preference, because 4K of a karaoke caption is disk spent on nothing the
/// appliance can show. Everything else is left to [`SORT`], which degrades instead of failing.
pub const FORMAT: &str = "bv*[height<=1080]+ba/b[height<=1080]/bv*+ba/b";

/// How to rank what [`FORMAT`] allowed.
///
/// Sorting rather than a longer `-f` fallback chain, and the difference matters: a chain that runs
/// out of alternatives fails the download, whereas a sort takes the nearest thing available and
/// lets the shape check afterwards say what was settled for. A song that arrived as VP9 is still a
/// song; a song that did not arrive is not.
pub const SORT: &str = "vcodec:h264,acodec:aac,res:1080,fps:30,ext:mp4:m4a";

/// `Artist - Title.mp4`, or `Title.mp4` when nothing knows an artist.
///
/// `%(FIELD&{} - |)s` is yt-dlp's conditional: emit `{} - ` with the field substituted when it has
/// one, and the empty default otherwise. Written this way rather than as `%(artist)s - %(title)s`
/// because the latter names a file `NA - Title.mp4` or ` - Title.mp4` depending on the version, and
/// the stem is the title of last resort — it has to be right when the tags are the thing that is
/// missing.
pub const OUTPUT_TEMPLATE: &str =
    "%(artist,album_artist,creator,uploader&{} - |)s%(track,title)s.%(ext)s";

/// The fields recorded per download, as one JSON object per line.
///
/// `after_move:` fires once the file has reached its final name, which is the only moment
/// `filepath` is worth writing down. The `%(.{…})j` form asks for a subset as JSON rather than the
/// whole info dict, which for a playlist is megabytes of thumbnails and format tables nothing here
/// reads.
pub const RECORD_TEMPLATE: &str = "after_move:%(.{id,filepath,artist,album_artist,creator,uploader,track,title,webpage_url,duration})j";

/// The same, for a run that is only pretending.
///
/// No `after_move:` prefix — that stage never happens under `--simulate` — and no `filepath`, since
/// nothing will be written. Kept in step with [`RECORD_TEMPLATE`] by a test.
pub const SIMULATE_TEMPLATE: &str =
    "%(.{id,artist,album_artist,creator,uploader,track,title,webpage_url,duration})j";

/// What the download archive is called, inside the destination folder.
///
/// Beside the videos rather than in a config directory, because the archive is a fact about *this
/// folder* — which songs are already in it — and a folder copied to another machine should carry
/// that with it.
pub const ARCHIVE_NAME: &str = ".km-fetched.txt";

/// What the record file is called, **relative to the destination**.
///
/// Three properties, all of them forced by `--print-to-file`'s file argument going through yt-dlp's
/// output-template machinery rather than being taken as a path:
///
/// * **Relative, so `-P` resolves it.** Given a long *absolute* path, `--trim-filenames` shortens it
///   by dropping directory components, which writes the record file one folder above the videos and
///   leaves the run reporting it fetched nothing at all, because that is where it looks.
/// * **No leading dot.** Sanitisation strips one, so `.records.jsonl` is written as `records.jsonl`
///   and a tool looking for the name it asked for finds nothing. It would rather be a hidden file;
///   it cannot be, so it is deleted as soon as it has been read instead.
/// * **`km-video-fetch-` in the name**, because for the moment between the download finishing and
///   the summary printing it does sit in somebody's folder of songs, and an interrupted run leaves
///   it there. It should say whose it is.
///
/// Unlike [`ARCHIVE_NAME`], which reaches `--download-archive` — an ordinary path argument that is
/// neither trimmed nor sanitised, and so keeps its dot and stays hidden.
pub const RECORDS_NAME: &str = "km-video-fetch-records.jsonl";

/// What a list of links to fetch is called, where it is called anything.
///
/// The extension on [`BATCH_NAME`], and the one a setup program associates with
/// `km-video-downloader` so that opening such a file opens the program. Written down once because
/// three unrelated places need it: the constant below, the page's file picker, and the installer.
///
/// **With the dot on it**, which is the form every one of those three wants — `ends_with`, an
/// `accept` attribute and a registry key alike.
pub const EXTENSION: &str = ".kmvf";

/// The list of URLs a folder can carry for itself.
///
/// **The third fact a destination folder is allowed to hold about itself**, beside [`ARCHIVE_NAME`]
/// and [`RECORDS_NAME`], and the same argument covers it: what to fetch *into this folder* belongs
/// with the folder, and a folder copied to another machine should carry it.
///
/// **Named rather than hidden**, unlike the archive: this one is a file somebody writes and edits by
/// hand, and a leading dot would make it invisible in exactly the file manager they would edit it
/// from. It says whose it is for `RECORDS_NAME`'s third reason.
///
/// **And it carries an extension of its own rather than `.txt`**, which is the one thing `.txt`
/// could not say: this is a document with a grammar — see [`crate::list`] — and an operating system
/// has no way to learn that from a name it shares with every other text file. `.kmvf` is what the
/// installer associates with `km-video-downloader`, so a list can be opened by double-clicking it.
///
/// **Two rules and not one, and they are easy to conflate.** *This* name is matched exactly, and
/// only in a destination folder: it is the list that folder carries for itself. The association is
/// matched by extension, on any such file anywhere, and means only that somebody opened it.
pub const BATCH_NAME: &str = "km-video-fetch.kmvf";

/// What a front end writes the links it was handed into, **relative to the destination**.
///
/// A file rather than arguments, and not for tidiness: a playlist pasted in as three hundred links
/// is an argv well past what Windows will accept.
pub const ASKED_NAME: &str = "km-video-fetch-asked.txt";

/// The two lists a marked run writes for itself, one per destination folder.
///
/// **A list that says what its lines are cannot be one yt-dlp run**, because `--yes-playlist` and
/// `--no-playlist` are properties of an invocation; see [`crate::list`]. So the list is sorted into
/// these and each is fetched on its own.
///
/// **One pair per destination folder rather than one pair for the run**, which is what makes the
/// names safe however many folders a list names: two runs never share a destination, so two runs
/// never share a file. It also puts a file left behind by an interrupted run beside the videos it
/// was about.
///
/// `km-video-fetch-` in the name for [`RECORDS_NAME`]'s third reason, and no leading dot for
/// [`BATCH_NAME`]'s: for the minute they exist they sit in somebody's folder of songs, and they
/// should say whose they are and be visible while they do.
///
/// **And `.txt` rather than [`BATCH_NAME`]'s `.kmvf`, which is now load-bearing.** That extension is
/// associated with `km-video-downloader`, and these are scratch: they sit in somebody's folder for
/// the length of one run, and an interrupted run leaves them there for good. A program that wrote a
/// double-clickable file into a folder of songs would be offering to reopen its own workings.
///
/// **Passed to `--batch-file` whole, not as a bare name — the opposite of [`RECORDS_NAME`].**
/// `--batch-file` is an ordinary path argument, resolved against the working directory and neither
/// trimmed nor sanitised, so it must carry its folder. `--print-to-file`'s goes through the
/// output-template machinery and must not. Two arguments that look alike and want opposite
/// treatment, which is why one test asserts both halves at once.
pub const SINGLES_NAME: &str = "km-video-fetch-singles.txt";

/// The other half of [`SINGLES_NAME`]: the lines that were said to be whole playlists.
pub const PLAYLISTS_NAME: &str = "km-video-fetch-playlists.txt";

/// The browsers yt-dlp can read cookies out of.
///
/// **Written down here rather than left to yt-dlp to reject**, so a front end can offer a list and
/// refuse a typo with its own words. The failure this avoids is specific: `--cookies-from-browser
/// chrom` is not a yt-dlp usage error but an *extraction* error, raised after the preflight has
/// passed and the download has begun, and it reads like the site refused rather than like a
/// misspelling.
///
/// **Sorted, because it is shown to a person.** Taken from `yt-dlp --help`; a version that grows a
/// tenth browser will still work if it is typed in full, since [`browser_is_known`] checks only the
/// part before the separators.
pub const COOKIE_BROWSERS: [&str; 9] = [
    "brave", "chrome", "chromium", "edge", "firefox", "opera", "safari", "vivaldi", "whale",
];

/// Whether yt-dlp will recognise this `--cookies-from-browser` value.
///
/// The full syntax is `BROWSER[+KEYRING][:PROFILE][::CONTAINER]`, and **only the browser is checked**
/// — a profile is a name or a path on somebody's own machine and there is nothing here that could
/// know it. Splitting on the separators is what lets `firefox:work` and `chrome+gnomekeyring` pass
/// while `frefox` does not.
#[must_use]
pub fn browser_is_known(value: &str) -> bool {
    let value = value.trim();
    let name = value
        .split(['+', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    COOKIE_BROWSERS.contains(&name.as_str())
}

/// One progress line, for a caller reading yt-dlp's output rather than showing it.
///
/// **The prefix is what makes the stream parseable at all.** yt-dlp writes this on stdout amongst
/// its ordinary `[youtube]` and `[download]` lines, so a reader needs to tell one kind from the
/// other; anything without the prefix is yt-dlp talking to a person and is passed through as such.
///
/// Three things about the values, all of them observed rather than assumed:
///
/// * **They are space-padded to a fixed width** — `pct=  2.4%` — so every field is trimmed before
///   it is parsed.
/// * **`Unknown` and `NA` are ordinary values.** Speed is unknown for the first second of every
///   download, and eta is `NA` on the final line.
/// * **`status=finished` ends one file, not the run.** A playlist emits it once per video.
///
/// `_percent_str` and its siblings rather than the raw byte counts because yt-dlp has already done
/// the arithmetic, including for a fragmented download where the total is only an estimate.
pub const PROGRESS_TEMPLATE: &str = "download:KMP status=%(progress.status)s \
     pct=%(progress._percent_str)s speed=%(progress._speed_str)s eta=%(progress._eta_str)s \
     title=%(info.title)s";

/// Pushes one plain-text argument.
///
/// A macro rather than a closure because the argv is built from a mixture of `&str` literals and
/// owned `OsString`s made from paths, and a closure holding a mutable borrow of the vector locks
/// out every direct `push` in between.
macro_rules! flag {
    ($args:ident, $value:expr) => {
        $args.push(OsString::from($value))
    };
}

/// Everything the argv depends on.
#[derive(Debug, Clone)]
pub struct Plan {
    /// URLs to fetch. Empty when [`Plan::from_file`] is set.
    pub targets: Vec<String>,
    /// A file of URLs, one per line.
    pub from_file: Option<PathBuf>,
    /// Expand a playlist rather than taking the one video from it.
    pub playlist: bool,
    /// Where the files go.
    pub out: PathBuf,
    /// At most this many items from a playlist.
    pub limit: Option<u32>,
    /// Where already-fetched ids are remembered, when they are.
    pub archive: Option<PathBuf>,
    /// A browser to take cookies from, for material that needs an account.
    pub cookies_from_browser: Option<String>,
    /// Mux subtitles into the file.
    pub subs: bool,
    /// Replace [`FORMAT`].
    pub format: Option<String>,
    /// Replace [`SORT`].
    pub sort: Option<String>,
    /// Ask what would happen and download nothing.
    pub dry_run: bool,
    /// Emit machine-readable progress lines beside the ordinary output.
    ///
    /// Off for a command line, which lets yt-dlp draw its own bar into the terminal it was given —
    /// that bar is better than anything reconstructed from a pipe. On for a caller that has no
    /// terminal to hand over and must read the progress instead, which is what a web page is. See
    /// [`PROGRESS_TEMPLATE`] and [`crate::fetch::Progress`].
    pub progress_lines: bool,
}

impl Plan {
    /// The archive path a plan uses by default, given its destination.
    #[must_use]
    pub fn default_archive(out: &Path) -> PathBuf {
        out.join(ARCHIVE_NAME)
    }

    /// Where [`RECORDS_NAME`] will actually land, which is what the caller reads back.
    #[must_use]
    pub fn records_path(out: &Path) -> PathBuf {
        out.join(RECORDS_NAME)
    }

    /// The list a destination folder carries for itself, if it carries one.
    ///
    /// `None` where the folder has no such file — including where the folder does not exist yet,
    /// which is an ordinary first run and not a fault.
    #[must_use]
    pub fn folders_own_list(out: &Path) -> Option<PathBuf> {
        let path = out.join(BATCH_NAME);
        path.is_file().then_some(path)
    }
}

/// Builds the whole yt-dlp command line, less the binary itself.
#[must_use]
pub fn argv(plan: &Plan) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();

    // Stated rather than inferred. yt-dlp's own default for a URL carrying `&list=` is to take the
    // whole playlist and warn about it, so somebody who pasted a link from a playlist page gets two
    // hundred songs they did not ask for. Here it is always one of the two, and always because it
    // was chosen.
    flag!(
        args,
        if plan.playlist {
            "--yes-playlist"
        } else {
            "--no-playlist"
        }
    );

    flag!(args, "-f");
    flag!(args, plan.format.as_deref().unwrap_or(FORMAT));
    flag!(args, "-S");
    flag!(args, plan.sort.as_deref().unwrap_or(SORT));

    // Both needed, and they are not the same thing: the first says what container to mux the
    // separate video and audio streams into, the second says what to do when the result still is
    // not MP4. `--remux-video` is a stream copy — it rewrites the container and never the picture.
    flag!(args, "--merge-output-format");
    flag!(args, "mp4");
    flag!(args, "--remux-video");
    flag!(args, "mp4");

    // The point of the whole exercise. `--embed-metadata` writes the container tags that
    // [`crate::probe`] reads back, so a video arrives in curation already knowing what it is; the
    // two `--parse-metadata` rules are what fills those tags in when the extractor reported the
    // song as a plain YouTube upload rather than as music.
    flag!(args, "--embed-metadata");
    flag!(args, "--parse-metadata");
    flag!(args, "%(track,title)s:%(meta_title)s");
    flag!(args, "--parse-metadata");
    flag!(
        args,
        "%(artist,album_artist,creator,uploader)s:%(meta_artist)s"
    );

    if plan.subs {
        flag!(args, "--embed-subs");
    }

    flag!(args, "-o");
    flag!(args, OUTPUT_TEMPLATE);

    // Forced on every platform, not only Windows. A corpus is fetched on one machine and packaged
    // on another — this box and the appliance — and a file that is named one thing here and another
    // thing there is a song whose stem, and therefore whose title, depends on where it landed.
    flag!(args, "--windows-filenames");
    flag!(args, "--trim-filenames");
    flag!(args, "120");

    flag!(args, "--no-overwrites");
    flag!(args, "--continue");
    flag!(args, "-N");
    flag!(args, "4");
    flag!(args, "--retries");
    flag!(args, "10");
    flag!(args, "--fragment-retries");
    flag!(args, "10");

    // Politeness, and self-interest: a playlist fetched flat out is how an address starts being
    // asked for a captcha, and a throttled download is slower than a paced one.
    flag!(args, "--sleep-requests");
    flag!(args, "1");
    flag!(args, "--sleep-interval");
    flag!(args, "2");
    flag!(args, "--max-sleep-interval");
    flag!(args, "6");

    // Progress as whole lines rather than as a bar rewritten with carriage returns. yt-dlp keeps
    // the terminal; this only stops the output turning into one very long line when it is piped
    // into a log.
    flag!(args, "--newline");

    // ...and for a caller with no terminal to give away, a second progress stream it can parse.
    // `--progress-delta` is what stops that stream being tens of lines a second on a fast link: the
    // page redraws once a second, so anything finer is work nobody sees.
    if plan.progress_lines {
        flag!(args, "--progress-delta");
        flag!(args, "0.5");
        flag!(args, "--progress-template");
        flag!(args, PROGRESS_TEMPLATE);
    }

    if let Some(limit) = plan.limit {
        flag!(args, "--playlist-items");
        args.push(OsString::from(format!("1:{limit}")));
    }

    if let Some(browser) = &plan.cookies_from_browser {
        flag!(args, "--cookies-from-browser");
        args.push(OsString::from(browser));
    }

    if plan.dry_run {
        flag!(args, "--simulate");
    }

    if let Some(archive) = &plan.archive {
        flag!(args, "--download-archive");
        args.push(archive.clone().into_os_string());
    }

    flag!(args, "-P");
    args.push(plan.out.clone().into_os_string());

    flag!(args, "--print-to-file");
    flag!(
        args,
        if plan.dry_run {
            SIMULATE_TEMPLATE
        } else {
            RECORD_TEMPLATE
        }
    );
    // Deliberately the bare name, resolved against the `-P` above. See [`RECORDS_NAME`].
    flag!(args, RECORDS_NAME);

    // Last, so that everything before it reads as configuration and a long argv can still be
    // skimmed for what it was actually asked to fetch.
    if let Some(file) = &plan.from_file {
        flag!(args, "--batch-file");
        args.push(file.clone().into_os_string());
    }
    for target in &plan.targets {
        args.push(OsString::from(target));
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan {
            targets: vec!["https://www.youtube.com/watch?v=abc".to_owned()],
            from_file: None,
            playlist: false,
            out: PathBuf::from("videos"),
            limit: None,
            archive: Some(PathBuf::from("videos/.km-fetched.txt")),
            cookies_from_browser: None,
            subs: false,
            format: None,
            sort: None,
            dry_run: false,
            progress_lines: false,
        }
    }

    fn strings(plan: &Plan) -> Vec<String> {
        argv(plan)
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// The value after `flag`, which is how a two-part option is asserted without depending on
    /// where in the argv it fell.
    fn value_of(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    }

    #[test]
    fn a_single_url_does_not_drag_in_its_playlist() {
        let args = strings(&plan());
        assert!(args.contains(&"--no-playlist".to_owned()));
        assert!(!args.contains(&"--yes-playlist".to_owned()));
        assert_eq!(args.last().unwrap(), "https://www.youtube.com/watch?v=abc");
    }

    #[test]
    fn a_playlist_is_expanded_only_when_asked() {
        let mut plan = plan();
        plan.playlist = true;
        let args = strings(&plan);
        assert!(args.contains(&"--yes-playlist".to_owned()));
        assert!(!args.contains(&"--no-playlist".to_owned()));
    }

    #[test]
    fn a_list_of_urls_becomes_a_batch_file() {
        let mut plan = plan();
        plan.targets.clear();
        plan.from_file = Some(PathBuf::from("songs.txt"));
        let args = strings(&plan);
        assert_eq!(
            value_of(&args, "--batch-file").as_deref(),
            Some("songs.txt")
        );
    }

    /// The profile is the reason this tool exists, so the two arguments that aim at it are asserted
    /// by value rather than by presence.
    #[test]
    fn it_asks_for_the_shape_packaging_wants() {
        let args = strings(&plan());
        assert_eq!(value_of(&args, "-f").as_deref(), Some(FORMAT));
        assert_eq!(value_of(&args, "-S").as_deref(), Some(SORT));
        assert_eq!(
            value_of(&args, "--merge-output-format").as_deref(),
            Some("mp4")
        );
        assert_eq!(value_of(&args, "--remux-video").as_deref(), Some("mp4"));
    }

    #[test]
    fn overrides_replace_the_defaults_rather_than_joining_them() {
        let mut plan = plan();
        plan.format = Some("bestvideo+bestaudio".to_owned());
        plan.sort = Some("res".to_owned());
        let args = strings(&plan);
        assert_eq!(
            value_of(&args, "-f").as_deref(),
            Some("bestvideo+bestaudio")
        );
        assert_eq!(value_of(&args, "-S").as_deref(), Some("res"));
        assert_eq!(args.iter().filter(|arg| *arg == "-f").count(), 1);
        assert_eq!(args.iter().filter(|arg| *arg == "-S").count(), 1);
    }

    /// Both of these change the *stream layout* of the file, which is the one thing the machine's
    /// decoder is strict about. See this module's header for why each is refused.
    #[test]
    fn nothing_extra_is_muxed_in_by_default() {
        let args = strings(&plan());
        assert!(!args.contains(&"--embed-thumbnail".to_owned()));
        assert!(!args.contains(&"--embed-subs".to_owned()));
        assert!(!args.contains(&"--restrict-filenames".to_owned()));
    }

    /// The progress stream is for a caller with no terminal, and a command line has one. Asserted
    /// in both directions because the cost of getting it wrong is asymmetric: an extra
    /// `--progress-template` in a terminal run replaces yt-dlp's bar with a wall of text, and a
    /// missing one in a watched run leaves a page with a bar that never moves.
    #[test]
    fn progress_lines_are_asked_for_rather_than_assumed() {
        let args = strings(&plan());
        assert!(!args.contains(&"--progress-template".to_owned()));
        assert!(!args.contains(&"--progress-delta".to_owned()));
        // Unconditional, and a different thing: it only stops the bar being one very long line.
        assert!(args.contains(&"--newline".to_owned()));

        let mut plan = plan();
        plan.progress_lines = true;
        let args = strings(&plan);
        assert_eq!(
            value_of(&args, "--progress-template").as_deref(),
            Some(PROGRESS_TEMPLATE)
        );
        assert_eq!(value_of(&args, "--progress-delta").as_deref(), Some("0.5"));
    }

    #[test]
    fn subs_adds_only_subtitles_never_a_thumbnail() {
        let mut plan = plan();
        plan.subs = true;
        let args = strings(&plan);
        assert!(args.contains(&"--embed-subs".to_owned()));
        assert!(!args.contains(&"--embed-thumbnail".to_owned()));
    }

    #[test]
    fn the_tags_the_scanner_reads_are_asked_for() {
        let args = strings(&plan());
        assert!(args.contains(&"--embed-metadata".to_owned()));
        assert_eq!(
            args.iter().filter(|arg| *arg == "--parse-metadata").count(),
            2
        );
        assert_eq!(value_of(&args, "-o").as_deref(), Some(OUTPUT_TEMPLATE));
    }

    #[test]
    fn a_limit_becomes_a_playlist_range() {
        let mut plan = plan();
        plan.limit = Some(12);
        assert_eq!(
            value_of(&strings(&plan), "--playlist-items").as_deref(),
            Some("1:12")
        );
    }

    #[test]
    fn the_archive_can_be_turned_off() {
        let args = strings(&plan());
        assert_eq!(
            value_of(&args, "--download-archive").as_deref(),
            Some("videos/.km-fetched.txt")
        );

        let mut plan = plan();
        plan.archive = None;
        assert!(!strings(&plan).contains(&"--download-archive".to_owned()));
    }

    /// Under `--simulate` the `after_move` stage never runs, so a template asking for it would
    /// record nothing at all and the dry run would report an empty list rather than what it found.
    #[test]
    fn a_dry_run_records_what_it_would_have_fetched() {
        let mut plan = plan();
        plan.dry_run = true;
        let args = strings(&plan);
        assert!(args.contains(&"--simulate".to_owned()));
        assert_eq!(
            value_of(&args, "--print-to-file").as_deref(),
            Some(SIMULATE_TEMPLATE)
        );
        assert!(!SIMULATE_TEMPLATE.contains("after_move"));
        assert!(!SIMULATE_TEMPLATE.contains("filepath"));
    }

    /// The two templates have to ask for the same fields, less the one that cannot exist, or a dry
    /// run reports something a real run does not.
    #[test]
    fn the_two_record_templates_stay_in_step() {
        let real = RECORD_TEMPLATE
            .trim_start_matches("after_move:")
            .replace("filepath,", "");
        assert_eq!(real, SIMULATE_TEMPLATE);
    }

    /// The whole point of the list: catching a typo here rather than letting yt-dlp raise it as an
    /// extraction error halfway through a download.
    #[test]
    fn a_browser_is_recognised_with_or_without_its_trimmings() {
        assert!(browser_is_known("firefox"));
        assert!(
            browser_is_known("  Chrome  "),
            "trimmed, and case does not matter"
        );
        assert!(browser_is_known("firefox:work"), "a named profile");
        assert!(browser_is_known("firefox::personal"), "a container");
        assert!(browser_is_known("chrome+gnomekeyring"), "a keyring");
        assert!(browser_is_known("chrome+gnomekeyring:Default"), "both");

        assert!(!browser_is_known("frefox"), "the typo this exists for");
        assert!(!browser_is_known(""));
        assert!(!browser_is_known(":work"), "a profile with no browser");
    }

    /// Every name offered has to be one yt-dlp will take, or the list teaches a mistake.
    #[test]
    fn every_offered_browser_is_accepted() {
        for browser in COOKIE_BROWSERS {
            assert!(browser_is_known(browser), "{browser}");
        }
    }

    #[test]
    fn cookies_are_passed_through_only_when_asked() {
        assert!(!strings(&plan()).contains(&"--cookies-from-browser".to_owned()));
        let mut plan = plan();
        plan.cookies_from_browser = Some("firefox".to_owned());
        assert_eq!(
            value_of(&strings(&plan), "--cookies-from-browser").as_deref(),
            Some("firefox")
        );
    }

    #[test]
    fn the_destination_is_where_it_was_asked_to_be() {
        let args = strings(&plan());
        assert_eq!(value_of(&args, "-P").as_deref(), Some("videos"));
        assert_eq!(
            value_of(&args, "--print-to-file").as_deref(),
            Some(RECORD_TEMPLATE)
        );
    }

    #[test]
    fn the_archive_sits_beside_the_videos() {
        assert_eq!(
            Plan::default_archive(Path::new("some/folder")),
            PathBuf::from("some/folder").join(ARCHIVE_NAME)
        );
        assert_eq!(
            Plan::records_path(Path::new("some/folder")),
            PathBuf::from("some/folder").join(RECORDS_NAME)
        );
    }

    /// Regression. `--print-to-file`'s file argument goes through yt-dlp's output-template
    /// machinery, so an absolute path there is *trimmed* by `--trim-filenames` — which shortens it
    /// by dropping directory components. The record file was written a folder above the videos and
    /// every run then reported fetching nothing, because that is where it looked. A bare name is
    /// resolved against `-P` and is too short to trim.
    #[test]
    fn the_record_file_is_named_relatively_so_trimming_cannot_move_it() {
        let args = strings(&plan());
        assert_eq!(
            value_of(&args, "--print-to-file").as_deref(),
            Some(RECORD_TEMPLATE)
        );

        // The name follows the template, and is the argument after it.
        let at = args
            .iter()
            .position(|arg| arg == RECORD_TEMPLATE)
            .expect("the template is in the argv");
        assert_eq!(args[at + 1], RECORDS_NAME);

        let name = Path::new(RECORDS_NAME);
        assert!(
            name.is_relative(),
            "an absolute path gets its directories trimmed away"
        );
        assert_eq!(
            name.components().count(),
            1,
            "one component, so there is nothing to trim off"
        );
        assert!(
            !RECORDS_NAME.starts_with('.'),
            "sanitisation strips a leading dot, and then the file is not where it was asked for"
        );
    }

    /// The counterpart of the regression above, asserted in one test so the asymmetry cannot be
    /// half-remembered: `--batch-file` is an ordinary path argument like `--download-archive`, and
    /// so must carry its folder, while `--print-to-file`'s goes through the output-template
    /// machinery and must not. Getting these the wrong way round writes one of the two files
    /// somewhere nothing looks for it.
    #[test]
    fn a_batch_file_keeps_its_folder_because_nothing_rewrites_that_argument() {
        let mut plan = plan();
        plan.targets.clear();
        plan.from_file = Some(PathBuf::from("videos").join(SINGLES_NAME));
        let args = strings(&plan);

        let batch = value_of(&args, "--batch-file").expect("the list is in the argv");
        assert!(
            Path::new(&batch).components().count() > 1,
            "a batch file is resolved against the working directory, not -P"
        );
        // The record file's name is the argument *after* the template, which is itself the value
        // after `--print-to-file`.
        let template = value_of(&args, "--print-to-file").expect("the template is in the argv");
        let record = value_of(&args, &template).expect("and the name follows it");
        assert_eq!(
            Path::new(&record).components().count(),
            1,
            "...and the record file is the other way round, or --trim-filenames moves it"
        );
    }

    /// Four names, four different files. The two a run writes for itself must never be mistaken for
    /// the one a person maintains by hand, nor for each other.
    #[test]
    fn the_lists_a_run_writes_cannot_be_mistaken_for_the_one_a_person_wrote() {
        let names = [BATCH_NAME, ASKED_NAME, SINGLES_NAME, PLAYLISTS_NAME];
        for (at, name) in names.iter().enumerate() {
            assert!(
                !names[at + 1..].contains(name),
                "{name} is used for two different things"
            );
            assert!(
                !name.starts_with('.'),
                "{name} is a file somebody may need to find"
            );
            assert!(
                name.starts_with("km-video-fetch"),
                "{name} says whose it is"
            );
        }
    }

    /// Only the list a person maintains carries the extension a double-click opens.
    ///
    /// The other three are scratch, written into somebody's folder of songs and left there by an
    /// interrupted run. `.kmvf` is associated with `km-video-downloader`, so one of those carrying
    /// it would be this program offering to reopen its own workings.
    #[test]
    fn nothing_this_program_writes_for_itself_is_a_file_a_double_click_would_open() {
        assert!(BATCH_NAME.ends_with(EXTENSION));
        for name in [
            ASKED_NAME,
            SINGLES_NAME,
            PLAYLISTS_NAME,
            RECORDS_NAME,
            ARCHIVE_NAME,
        ] {
            assert!(
                !name.ends_with(EXTENSION),
                "{name} is scratch and must not look like a document"
            );
        }
    }

    /// The counterpart: `--download-archive` is an ordinary path argument, neither trimmed nor
    /// sanitised, so it keeps both its directory and its leading dot.
    #[test]
    fn the_archive_keeps_its_dot_because_nothing_rewrites_that_argument() {
        assert!(ARCHIVE_NAME.starts_with('.'));
        let args = strings(&plan());
        assert_eq!(
            value_of(&args, "--download-archive").as_deref(),
            Some("videos/.km-fetched.txt")
        );
    }
}
