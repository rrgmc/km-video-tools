//! A list of links, and the three things a line of one may say about itself.
//!
//! # Why a list has a grammar at all
//!
//! yt-dlp's `--batch-file` is a list of URLs and nothing else, and `--yes-playlist`/`--no-playlist`
//! and `-P` are properties of an *invocation*. So a single yt-dlp run cannot be told that one line
//! of its list is a whole playlist and the next is one video, nor that two of them belong in
//! different folders. There is no per-URL form of either option to reach for.
//!
//! `docs/design.md` states the rule this refines: whether a URL means one video or a playlist is
//! stated rather than guessed. The run says what a line means when the line does not, and a line may
//! say. What it costs is that a marked list is several yt-dlp runs rather than one — see
//! [`crate::fetch`], where the splitting happens.
//!
//! # The grammar
//!
//! A line is **flags, then a URL**, and the flags are spelled as the command-line flags they
//! override, because that is the one spelling somebody reading the file already knows:
//!
//! ```text
//! https://youtu.be/aaaaaaaaaaa
//! --playlist https://www.youtube.com/playlist?list=PLxxxx
//! --no-playlist https://youtu.be/bbbbbbbbbbb?list=PLyyyy
//! --out anime https://youtu.be/ccccccccccc
//! --playlist --out anime/openings https://www.youtube.com/playlist?list=PLzzzz
//! ```
//!
//! **Three words, and deliberately not "whatever yt-dlp takes".** A line that could carry a format
//! selector or a cookie browser is a much larger promise than this, and every one of them would have
//! to become another axis the run is split along.
//!
//! # A header, for the things that are true of the whole list
//!
//! That refusal is about a *line*, and it does not reach a setting that is true of the run. A cookie
//! browser is one field of an [`crate::args::Plan`] shared by every run a list produces, so it
//! splits nothing — and a folder that already says what to fetch into it should be able to say how.
//!
//! ```text
//! --cookies-from-browser firefox
//! --normalize
//! --limit 50
//!
//! https://youtu.be/aaaaaaaaaaa
//! --out anime https://youtu.be/bbbbbbbbbbb
//! ```
//!
//! **The header is every line before the first URL**, blanks and comments included, and it ends at
//! the first line that is not a setting. One rule, and it is the rule that makes a bare `--playlist`
//! unambiguous: at the top it is the run's answer for lines that do not say, and in front of a URL
//! it is that line's. [`read_entry`] already reads a line that is nothing but a marker as no line at
//! all, so the two never met.
//!
//! Each is spelled as the `km-video-fetch` flag it overrides, for the reason the markers above are:
//! it is the one spelling somebody reading the file already knows. [`Settings`] lists them.
//!
//! **A line the header does not understand ends the header** rather than being refused, which is
//! [`read_entry`]'s own treatment of a word that is not a marker. `--limit abc` becomes a URL, and
//! yt-dlp says it is not one — loud, in the words of the program that would know, and without this
//! module growing a way to fail that [`read`] has spent its whole life not having.
//!
//! # What a header may not say, which is the more interesting half
//!
//! * **`--out`** — the destination is where the file lives. A list that moved its own folder could
//!   not be copied anywhere. The per-line `--out` already exists and is relative to the run's own.
//! * **`--dry-run`, `--strict`, `--show-command`** — properties of an invocation, not of a folder.
//!   A folder that always simulates is a folder that never downloads.
//! * **`--from-file`** — it *is* the file.
//! * **`--yt-dlp`** — a fact about a machine, not about a folder. A list copied to another machine
//!   would carry a path that does not exist there.
//!
//! # A file with no markers is not this tool's file
//!
//! It is an ordinary yt-dlp batch file, and [`crate::fetch`] hands it over untouched rather than
//! reading and rewriting it. That is what keeps a `km-video-fetch.kmvf` somebody maintains by hand
//! from being rewritten behind their back, and what keeps a list holding things this module does not
//! model — a `;` comment, an option yt-dlp itself understands — working exactly as it did.
//!
//! Which is also why the comment rule here is **`#`, `;` and `]`**, all three, rather than the `#`
//! that is the only one anybody writes. Skipping a line and *re-emitting* it are different mistakes:
//! passing a `;` line through costs nothing, and writing it back out as a URL would produce an
//! extraction error for a line yt-dlp was always going to ignore.

use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

/// The characters that start a comment, taken from yt-dlp's own `--batch-file`.
const COMMENT: [char; 3] = ['#', ';', ']'];

/// What a folder named by a line may not contain, on any platform. See [`destination`].
///
/// The two characters that decide how a path is *parsed*, and that only Windows parses.
const NOT_IN_A_NAME: [char; 2] = ['\\', ':'];

/// Said of a line that is a whole playlist.
pub const EXPAND_MARKER: &str = "--playlist";

/// Said of a line that is one video, in a run that expands by default.
pub const SINGLE_MARKER: &str = "--no-playlist";

/// Said of a line that goes somewhere under the folder the run was pointed at.
pub const OUT_MARKER: &str = "--out";

/// One line of a list: a link, and what it was said to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The link, with any markers taken off.
    pub url: String,
    /// `Some(true)` said *expand this*, `Some(false)` said *just this video*, and `None` said
    /// nothing at all and takes the run's own answer.
    pub expand: Option<bool>,
    /// A folder under the run's own, exactly as written. `None` for the run's own.
    ///
    /// Kept as text rather than resolved here because resolving it needs the destination, which is
    /// the run's business and not the line's. [`destination`] is where the two meet.
    pub out: Option<String>,
}

impl Entry {
    /// A line that says nothing but its link.
    #[must_use]
    pub fn plain(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            expand: None,
            out: None,
        }
    }

    /// Whether this line expands, given what the run says for a line that did not say.
    #[must_use]
    pub fn expands(&self, by_default: bool) -> bool {
        self.expand.unwrap_or(by_default)
    }
}

/// What a header said about the whole list.
///
/// Every field is what the same-named `km-video-fetch` flag means, and every one of them is a single
/// value shared by every run the list produces — which is what separates these from the per-line
/// markers and is why they cost nothing. See this module's header for the four that are refused.
///
/// **How this meets what was asked for on the command line is not decided here.** It is
/// [`crate::fetch::Request::apply_list_settings`], because a `Request` is the only thing holding all
/// of these at once — and because *who* applies it is load-bearing: a front end that shows these to
/// somebody before they press Fetch must not have them folded in a second time behind the form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `--playlist`: expand a playlist, for lines that do not say for themselves.
    pub playlist: bool,
    /// `--subs`: mux subtitles in.
    pub subs: bool,
    /// `--normalize`: re-encode anything that lands outside the profile.
    pub normalize: bool,
    /// `--no-archive`: fetch what is already in the folder's archive again.
    pub no_archive: bool,
    /// `--limit N`: take at most this many items from a playlist.
    pub limit: Option<u32>,
    /// `--cookies-from-browser BROWSER`: which browser's cookies to use.
    ///
    /// Not checked here. An unknown one is [`crate::fetch::fetch`]'s to refuse, in the one place
    /// that already words it well, and it must be refused the same whichever way it arrived.
    pub cookies_from_browser: Option<String>,
    /// `--format SELECTOR`: replace the format selector.
    pub format: Option<String>,
    /// `--sort ORDER`: replace the format sort order.
    pub sort: Option<String>,
}

/// Reads one settings line into `settings`, and says whether it was one.
///
/// `false` ends the header — see this module's doc for why that is a reinterpretation rather than a
/// refusal. A valued setting with nothing after it is not a setting, so a bare `--limit` ends the
/// header rather than quietly meaning nothing.
fn read_setting(settings: &mut Settings, line: &str) -> bool {
    let (word, value) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(word, rest)| (word, rest.trim()));

    match (word, value) {
        (EXPAND_MARKER, "") => settings.playlist = true,
        ("--subs", "") => settings.subs = true,
        ("--normalize", "") => settings.normalize = true,
        ("--no-archive", "") => settings.no_archive = true,
        ("--limit", value) if !value.is_empty() => match value.parse() {
            Ok(limit) => settings.limit = Some(limit),
            Err(_) => return false,
        },
        ("--cookies-from-browser", value) if !value.is_empty() => {
            settings.cookies_from_browser = Some(value.to_owned());
        }
        ("--format", value) if !value.is_empty() => settings.format = Some(value.to_owned()),
        ("--sort", value) if !value.is_empty() => settings.sort = Some(value.to_owned()),
        _ => return false,
    }
    true
}

/// Splits a list into what its header said and everything after it.
///
/// **The one function that must be reached from every path that reads a list**, and the reason is
/// the web UI rather than this one: the page merges several lists into the file it hands yt-dlp, and
/// a header line surviving that merge would be fetched as a URL. So [`merge`] calls this, and
/// therefore so do [`entries_in`] and [`read`].
fn split_header(text: &str) -> (Settings, &str) {
    let mut settings = Settings::default();
    let mut header = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        // Blanks and comments do not end the header; they are also nothing to the body.
        if !(trimmed.is_empty()
            || trimmed.starts_with(COMMENT)
            || read_setting(&mut settings, trimmed))
        {
            break;
        }
        header += line.len();
    }
    (settings, &text[header..])
}

/// What a list said about itself, and the links in it.
#[must_use]
pub fn parse(text: &str) -> (Settings, Vec<Entry>) {
    (split_header(text).0, entries_in(text))
}

/// What a list on disk said about itself, and nothing where it says nothing or cannot be read.
///
/// [`read`]'s rule, for [`read`]'s reason: a file nobody can read asks for nothing.
#[must_use]
pub fn settings_of(path: &Path) -> Settings {
    std::fs::read_to_string(path).map_or_else(|_| Settings::default(), |text| split_header(&text).0)
}

/// Reads one line, or `None` for a blank, a comment, or a marker with nothing after it.
///
/// **One of the two functions in this repository that know what a marker looks like**, the other
/// being [`line_of`]. Everything downstream takes an [`Entry`], so changing the spelling is changing
/// these two and the literals in their round-trip test, and nothing else at all.
///
/// A marker is only a marker at the front of a line: the loop stops at the first word that is not
/// one, and everything from there is the URL. That is what lets a link carrying `--playlist` as a
/// query parameter through unharmed, and it is why the URL is taken as the rest of the line rather
/// than as the next word — a URL may contain a space, and nothing here should be deciding what to
/// do about that.
fn read_entry(line: &str) -> Option<Entry> {
    let mut rest = line.trim();
    if rest.is_empty() || rest.starts_with(COMMENT) {
        return None;
    }

    let mut entry = Entry {
        url: String::new(),
        expand: None,
        out: None,
    };

    // No whitespace left means no marker can be starting here: the whole of `rest` is the URL.
    while let Some((word, tail)) = rest.split_once(char::is_whitespace) {
        let tail = tail.trim_start();
        match word {
            EXPAND_MARKER => entry.expand = Some(true),
            SINGLE_MARKER => entry.expand = Some(false),
            OUT_MARKER => {
                // The folder is the next word and the URL is what follows it. A `--out` with
                // nothing after it names no link, and a line with no link is not a line.
                let (folder, after) = tail.split_once(char::is_whitespace)?;
                entry.out = Some(folder.to_owned());
                rest = after.trim_start();
                continue;
            }
            _ => break,
        }
        rest = tail;
    }

    // A line that is nothing but a marker named no link, and the loop above cannot tell: it stops
    // at the first word with no whitespace after it, and a bare `--playlist` is exactly that.
    let url = rest.trim();
    if url.is_empty() || [EXPAND_MARKER, SINGLE_MARKER, OUT_MARKER].contains(&url) {
        return None;
    }
    entry.url = url.to_owned();
    Some(entry)
}

/// How one entry is written back down.
///
/// The inverse of [`read_entry`], and held to it by a test: a split list is written back out for
/// yt-dlp to read, and a line that did not survive the round trip would be a link fetched with the
/// wrong answer or not at all.
#[must_use]
pub fn line_of(entry: &Entry) -> String {
    let mut line = String::new();
    match entry.expand {
        Some(true) => line.push_str(EXPAND_MARKER),
        Some(false) => line.push_str(SINGLE_MARKER),
        None => {}
    }
    if let Some(out) = &entry.out {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(OUT_MARKER);
        line.push(' ');
        line.push_str(out);
    }
    if !line.is_empty() {
        line.push(' ');
    }
    line.push_str(&entry.url);
    line
}

/// Writes a list of links out for yt-dlp, one per line and **markers stripped**.
///
/// The counterpart of [`write`]. `write` is for a list this tool will read again — the markers are
/// the point of it. This one is for a list *yt-dlp*
/// will read, by which time the markers have already been spent deciding which run this is: yt-dlp
/// has no idea what `--playlist` at the front of a line means and would take the whole line as a URL,
/// which fails as `is not a valid URL` on a line that named a perfectly good one.
pub fn write_urls(path: &Path, entries: &[Entry]) -> Result<()> {
    let text: String = entries
        .iter()
        .map(|entry| entry.url.clone() + "\n")
        .collect();
    std::fs::write(path, text)?;
    Ok(())
}

/// The links a block of text holds, in the order it holds them, the first mention of each winning.
///
/// **First mention rather than last**, because a list is read top to bottom and the thing said first
/// is the thing somebody meant. The alternative makes a link's meaning depend on how far down the
/// file the reader has got.
#[must_use]
pub fn entries_in(text: &str) -> Vec<Entry> {
    merge(&[text])
}

/// The same, from a file.
///
/// **An unreadable file is an empty list rather than an error**, the same answer
/// [`crate::run::read_records`] gives, and for a reason particular to this one: it is read in order
/// to find out whether a list carries markers, and a list that cannot be read carries none. Whether
/// the file exists at all is yt-dlp's to complain about, in its own words, at the moment it is
/// handed the path.
#[must_use]
pub fn read(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path).map_or_else(|_| Vec::new(), |text| entries_in(&text))
}

/// Everything several blocks of text hold, the first mention of a link winning across all of them.
///
/// **Not `Vec::dedup`**, which drops only *consecutive* equals and so lets a link present in two of
/// the sources through twice.
///
/// **A link is the same link only where it is going to the same place.** The same video named for
/// two folders is two files and was asked for twice on purpose — that is what asking for two folders
/// means — so the destination is half of what makes a duplicate a duplicate. Two lines naming it for
/// the *same* folder are a duplicate whatever else they say, and the first of them wins.
#[must_use]
pub fn merge(sources: &[&str]) -> Vec<Entry> {
    let mut all: Vec<Entry> = Vec::new();
    for text in sources {
        // **Each source's own header, dropped before its links are read.** Every source here is a
        // whole list — a textarea, a picked file, a folder's own — and any of them may carry one.
        let (_, body) = split_header(text);
        for line in body.lines() {
            let Some(entry) = read_entry(line) else {
                continue;
            };
            if !all
                .iter()
                .any(|already| already.url == entry.url && already.out == entry.out)
            {
                all.push(entry);
            }
        }
    }
    all
}

/// Writes a list back out, markers and all.
pub fn write(path: &Path, entries: &[Entry]) -> Result<()> {
    let text: String = entries.iter().map(|entry| line_of(entry) + "\n").collect();
    std::fs::write(path, text)?;
    Ok(())
}

/// Where a line's `--out` actually lands.
///
/// **Refused rather than quietly clamped when it would leave the folder the run was pointed at.** A
/// list is a file, and a file can come from somewhere other than the person running the fetch — but
/// the argument holds with nobody being adversarial at all: `--out ../songs` in a list copied
/// between two machines writes into whatever happens to sit beside the destination on the second
/// one, which is a mess to discover afterwards and free to refuse now.
///
/// The rule is in two halves. **Every component must be an ordinary name**, which turns down `..`, a
/// leading separator, a drive letter, a `C:relative` and a UNC prefix without naming any of them —
/// and **the text may not contain a backslash or a colon**, checked before anything parses it, which
/// is what makes the first half mean the same thing wherever it runs.
///
/// **The second half is not belt and braces; the first half alone is platform-dependent.** `\` and
/// `:` are the two characters that decide how a path is *parsed*, and only Windows parses them:
/// `Path::components` on Unix splits on `/` and nothing else, so `anime\openings` there is one
/// perfectly ordinary name, while Windows makes it two nested folders — and *neither* answer looks
/// wrong to a check that asks the components, because on Windows the separator has been eaten by
/// then and both names come back spotless. A list is a file that moves between machines, and one
/// that sorts a corpus two ways depending on where it ran is the same fault `--windows-filenames` is
/// forced on every platform to prevent — see [`crate::args::argv`].
///
/// On Windows this is very nearly a no-op, those shapes being refused as a `Prefix` or a `RootDir`
/// already. The one thing it genuinely adds there is `ab:cd`, which is an ordinary component to
/// `Path` and an illegal filename to Windows, `:` being what opens an alternate data stream.
///
/// **Deliberately not the rest of Windows' illegal set** — `< > " | ? *`, trailing dots, `CON` and
/// its siblings. Those decide whether a name is *valid*, not how a path is *structured*: `--out a?b`
/// fails at `create_dir_all` in the operating system's own words, which is a fine way to find out,
/// whereas `--out a\b` quietly means something else. Structure is the line worth drawing here;
/// drawing it wider would be inventing a filename policy this module has no business owning.
pub fn destination(root: &Path, said: &str) -> Result<PathBuf> {
    let said = said.trim();
    // **The text, before anything parses it.** Asking this of the *components* would be asking the
    // parser, and the parser is the thing that differs: Windows takes the backslash in
    // `anime\openings` as a separator and hands back two spotless ordinary names, while Unix hands
    // back one. Only the string itself is the same on both.
    let ordinary = !said.is_empty()
        && !said.contains(NOT_IN_A_NAME)
        && Path::new(said)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !ordinary {
        bail!(
            "`{said}` is not a folder inside {} — a list may only name folders under the one the \
             fetch was pointed at",
            root.display()
        );
    }
    Ok(root.join(said))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything a header may say, in one file, read off in one pass.
    #[test]
    fn a_header_says_what_is_true_of_the_whole_list() {
        let (settings, entries) = parse(
            "--playlist\n\
             --subs\n\
             --normalize\n\
             --no-archive\n\
             --limit 50\n\
             --cookies-from-browser firefox:work\n\
             --format bv*+ba/b\n\
             --sort res:1080,fps\n\
             \n\
             https://example.invalid/a\n",
        );

        assert_eq!(
            settings,
            Settings {
                playlist: true,
                subs: true,
                normalize: true,
                no_archive: true,
                limit: Some(50),
                cookies_from_browser: Some("firefox:work".to_owned()),
                format: Some("bv*+ba/b".to_owned()),
                sort: Some("res:1080,fps".to_owned()),
            }
        );
        assert_eq!(entries, vec![Entry::plain("https://example.invalid/a")]);
    }

    /// **The assertion the whole feature rests on.** A header line that survived into the links
    /// would be handed to yt-dlp as a URL, and every path that reads a list goes through `merge`.
    #[test]
    fn a_header_is_never_mistaken_for_a_link() {
        let list = "--cookies-from-browser firefox\n\nhttps://example.invalid/a\n";
        assert_eq!(
            entries_in(list),
            vec![Entry::plain("https://example.invalid/a")]
        );
        assert_eq!(
            merge(&[list, "https://example.invalid/b\n"]),
            vec![
                Entry::plain("https://example.invalid/a"),
                Entry::plain("https://example.invalid/b"),
            ],
            "and every source gets its own header taken off"
        );
    }

    /// The header is only the top of the file, which is what keeps a bare `--playlist` from
    /// meaning two things at once: at the top it is the run's answer, in front of a URL it is that
    /// line's. Blanks and comments do not end it.
    #[test]
    fn the_header_ends_at_the_first_link_and_a_marker_after_it_is_a_marker() {
        let (settings, entries) = parse(
            "# a heading\n\
             \n\
             --limit 3\n\
             ; and a comment\n\
             https://example.invalid/a\n\
             --playlist https://example.invalid/list\n",
        );

        assert_eq!(settings.limit, Some(3));
        assert!(
            !settings.playlist,
            "the marker below the first link is that line's, not the run's"
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].expand, Some(true));
    }

    /// A bare `--playlist` at the top is the one spelling that had to be checked in both places.
    #[test]
    fn a_bare_marker_at_the_top_is_the_runs_answer() {
        let (settings, entries) = parse("--playlist\nhttps://example.invalid/list\n");
        assert!(settings.playlist);
        assert_eq!(entries, vec![Entry::plain("https://example.invalid/list")]);
    }

    /// A line the header cannot use ends it rather than being refused — see the module doc. The
    /// cost is that a typo becomes a URL, and yt-dlp says so in words that name it.
    #[test]
    fn a_line_the_header_does_not_understand_ends_the_header() {
        for bad in [
            "--limit abc",
            "--limit",
            "--cookies-from-browser",
            "--nonsense",
            "--sort",
        ] {
            let list = format!("{bad}\nhttps://example.invalid/a\n");
            let (settings, entries) = parse(&list);
            assert_eq!(settings, Settings::default(), "{bad} set nothing");
            assert_eq!(
                entries.len(),
                2,
                "{bad} became a link, as yt-dlp will report"
            );
            assert_eq!(entries[0].url, bad);
        }
    }

    /// The compatibility half: a list with no header is what it has always been.
    #[test]
    fn a_list_with_no_header_says_nothing_and_reads_as_it_always_did() {
        let list =
            "# a heading\n\nhttps://example.invalid/a\n--out anime https://example.invalid/b\n";
        let (settings, entries) = parse(list);
        assert_eq!(settings, Settings::default());
        assert_eq!(entries, entries_in(list));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].out.as_deref(), Some("anime"));
    }

    /// The compatibility case, and the one that matters most: an ordinary list of links is an
    /// ordinary list of links, and nothing here has an opinion about it.
    #[test]
    fn a_plain_list_says_nothing_about_playlists_and_is_left_alone() {
        let entries = entries_in("https://example.invalid/a\nhttps://example.invalid/b\n");
        assert_eq!(
            entries,
            vec![
                Entry::plain("https://example.invalid/a"),
                Entry::plain("https://example.invalid/b"),
            ]
        );
        assert!(entries.iter().all(|entry| entry.expand.is_none()));
        assert!(entries.iter().all(|entry| entry.out.is_none()));
    }

    #[test]
    fn a_marked_line_says_which_it_is_and_the_marker_is_not_part_of_the_url() {
        let entries = entries_in(
            "--playlist https://example.invalid/list\n\
             --no-playlist https://example.invalid/one\n\
             --out anime https://example.invalid/two\n\
             --playlist --out anime/openings https://example.invalid/three\n\
             --out jpop --playlist https://example.invalid/four\n",
        );
        assert_eq!(entries[0].url, "https://example.invalid/list");
        assert_eq!(entries[0].expand, Some(true));
        assert_eq!(entries[0].out, None);

        assert_eq!(entries[1].expand, Some(false));
        assert_eq!(entries[2].out.as_deref(), Some("anime"));
        assert_eq!(entries[2].url, "https://example.invalid/two");
        assert_eq!(
            entries[2].expand, None,
            "--out says nothing about playlists"
        );

        assert_eq!(entries[3].expand, Some(true));
        assert_eq!(entries[3].out.as_deref(), Some("anime/openings"));
        assert_eq!(entries[3].url, "https://example.invalid/three");

        assert_eq!(entries[4].expand, Some(true), "and in either order");
        assert_eq!(entries[4].out.as_deref(), Some("jpop"));
    }

    /// All three, because yt-dlp reads all three that way and this list is written back out for
    /// yt-dlp to read. Skipping a line it would skip costs nothing; re-emitting one as a URL is an
    /// extraction error for a line nobody meant to fetch.
    #[test]
    fn blanks_and_comments_are_skipped_the_way_yt_dlp_skips_them() {
        let entries = entries_in(
            "# a heading\n\
             \n\
             https://example.invalid/a\n\
             ; another kind of comment\n\
             ] and the third kind\n\
             \t\n\
             --playlist https://example.invalid/b\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].url, "https://example.invalid/a");
        assert_eq!(entries[1].url, "https://example.invalid/b");
    }

    /// The list yt-dlp is handed is not the list this tool reads. Found by running it: a generated
    /// file written with its markers on made yt-dlp report
    /// `'--playlist https://…' is not a valid URL` for a line that named a perfectly good one.
    #[test]
    fn the_list_handed_to_yt_dlp_has_the_markers_taken_off_again() {
        let dir = std::env::temp_dir().join("km-video-list-urls");
        std::fs::create_dir_all(&dir).expect("a folder to write in");
        let path = dir.join("urls.txt");

        write_urls(
            &path,
            &[
                Entry {
                    url: "https://example.invalid/a".to_owned(),
                    expand: Some(true),
                    out: Some("anime".to_owned()),
                },
                Entry::plain("https://example.invalid/b"),
            ],
        )
        .expect("write the list");

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "https://example.invalid/a\nhttps://example.invalid/b\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same video in two folders is two files, and was asked for twice on purpose — which is
    /// what asking for two folders means. So a destination is half of what makes a duplicate.
    #[test]
    fn the_same_link_for_two_folders_is_two_things_to_fetch() {
        let entries = entries_in(
            "https://example.invalid/a\n\
             --out anime https://example.invalid/a\n\
             --out anime https://example.invalid/a\n",
        );
        assert_eq!(entries.len(), 2, "two folders, not three lines");
        assert_eq!(entries[0].out, None);
        assert_eq!(entries[1].out.as_deref(), Some("anime"));
    }

    /// Two lines naming one link for one folder are one thing to fetch, and the first of them says
    /// what it is: a list is read top to bottom, and the alternative makes a link's meaning depend
    /// on how far down the file the reader has got.
    #[test]
    fn a_repeated_link_keeps_the_first_thing_said_about_it() {
        let entries = entries_in(
            "--playlist https://example.invalid/a\n\
             --no-playlist https://example.invalid/a\n",
        );
        assert_eq!(entries.len(), 1, "asked for once");
        assert_eq!(entries[0].expand, Some(true));

        let entries = entries_in(
            "--out anime https://example.invalid/a\n\
             --playlist --out anime https://example.invalid/a\n",
        );
        assert_eq!(entries.len(), 1, "and the same folder is the same folder");
        assert_eq!(entries[0].expand, None);
    }

    /// `Vec::dedup` drops only *consecutive* equals, so a link in two of the sources survived it.
    /// This is the regression.
    #[test]
    fn a_link_in_two_places_is_kept_once_and_keeps_its_first_marker() {
        let entries = merge(&[
            "--playlist https://example.invalid/a\nhttps://example.invalid/b\n",
            "https://example.invalid/a\nhttps://example.invalid/c\n",
        ]);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].expand, Some(true), "the first mention wins");
        assert_eq!(entries[1].url, "https://example.invalid/b");
        assert_eq!(entries[2].url, "https://example.invalid/c");
    }

    /// The round trip is what holds the parser and the writer together, and it is the one test that
    /// has to be revisited if the markers are ever spelled differently.
    #[test]
    fn an_entry_written_back_out_reads_as_the_same_entry() {
        let cases = [
            Entry::plain("https://example.invalid/a"),
            Entry {
                url: "https://example.invalid/b".to_owned(),
                expand: Some(true),
                out: None,
            },
            Entry {
                url: "https://example.invalid/c".to_owned(),
                expand: Some(false),
                out: None,
            },
            Entry {
                url: "https://example.invalid/d".to_owned(),
                expand: None,
                out: Some("anime".to_owned()),
            },
            Entry {
                url: "https://example.invalid/e".to_owned(),
                expand: Some(true),
                out: Some("anime/openings".to_owned()),
            },
        ];
        for entry in cases {
            let line = line_of(&entry);
            assert_eq!(
                read_entry(&line).as_ref(),
                Some(&entry),
                "{line} did not survive the round trip"
            );
        }
    }

    /// A marker is only a marker at the front of a line. `docs/design.md` says a URL's meaning is
    /// stated rather than guessed, and a link that merely *contains* one of these words has stated
    /// nothing.
    #[test]
    fn a_marker_is_only_a_marker_at_the_front_of_a_line() {
        let entries = entries_in(
            "https://example.invalid/a?x=--playlist\n\
             https://example.invalid/b --playlist\n",
        );
        assert_eq!(entries[0].url, "https://example.invalid/a?x=--playlist");
        assert_eq!(entries[0].expand, None);
        assert_eq!(
            entries[1].url, "https://example.invalid/b --playlist",
            "the URL is the rest of the line, trailing words and all"
        );
        assert_eq!(entries[1].expand, None);
    }

    /// Guards `docs/design.md`'s rule against a future helpful sniffer: a link that looks exactly
    /// like a playlist has still not said it is one.
    #[test]
    fn a_url_that_merely_looks_like_a_playlist_is_still_not_guessed_about() {
        let entries = entries_in("https://www.youtube.com/playlist?list=PLxxxx\n");
        assert_eq!(entries[0].expand, None);
    }

    #[test]
    fn a_line_with_a_marker_and_no_link_is_not_a_line() {
        assert_eq!(read_entry("--playlist"), None);
        assert_eq!(read_entry("--playlist   "), None);
        assert_eq!(read_entry("--out anime"), None);
        assert_eq!(read_entry("--out"), None);
    }

    /// One rule, every shape. Written out at length because each of these is a different way of
    /// leaving the folder and a check that caught four of the five would look correct.
    ///
    /// **Every case here is refused on whichever machine the suite runs on**, which is not what
    /// `Component::Normal` gives on its own. Unix splits a path on `/` and nothing else, so the four
    /// written with backslashes are single ordinary names there and pass any check that trusts the
    /// platform's parser. Do not simplify this back to trusting it.
    #[test]
    fn a_destination_is_a_folder_under_the_one_asked_for_and_never_beside_it() {
        let root = Path::new("songs");
        assert_eq!(
            destination(root, "anime").unwrap(),
            PathBuf::from("songs").join("anime")
        );
        assert_eq!(
            destination(root, "anime/openings").unwrap(),
            PathBuf::from("songs").join("anime/openings")
        );

        for refused in [
            "..",
            "../beside",
            "anime/../..",
            "/etc",
            r"\windows",
            r"C:\Windows",
            "C:relative",
            r"\\server\share",
            // One name on Unix and two nested folders on Windows, which is why the rule names
            // these characters rather than leaving them to `Component::Normal`.
            r"anime\openings",
            // An ordinary component to `Path` even on Windows, and an illegal filename to Windows
            // itself: `:` is what opens an alternate data stream.
            "ab:cd",
            "",
            "   ",
        ] {
            assert!(
                destination(root, refused).is_err(),
                "{refused} should not be reachable from a list"
            );
        }
    }

    #[test]
    fn a_written_list_is_read_back_as_what_was_written() {
        let dir = std::env::temp_dir().join("km-video-list-test");
        std::fs::create_dir_all(&dir).expect("a folder to write in");
        let path = dir.join("list.txt");

        let entries = vec![
            Entry::plain("https://example.invalid/a"),
            Entry {
                url: "https://example.invalid/b".to_owned(),
                expand: Some(true),
                out: Some("anime".to_owned()),
            },
        ];
        write(&path, &entries).expect("write the list");
        assert_eq!(read(&path), entries);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of [`read`]'s promise: this is asked in order to find out whether a list
    /// carries markers, and a list that is not there carries none.
    #[test]
    fn a_list_that_is_not_there_is_no_markers_rather_than_a_failure() {
        assert_eq!(read(Path::new("no-such-file-anywhere.txt")), Vec::new());
    }
}
