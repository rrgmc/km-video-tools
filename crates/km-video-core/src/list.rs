//! A list of links, and the three things a line of one may say about itself.
//!
//! # Why a list has a grammar at all
//!
//! yt-dlp's `--batch-file` is a list of URLs and nothing else, and `--yes-playlist`/`--no-playlist`
//! and `-P` are properties of an *invocation*. So a single yt-dlp run cannot be told that one line
//! of its list is a whole playlist and the next is one video, nor that two of them belong in
//! different folders. There is no per-URL form of either option to reach for.
//!
//! `docs/design.md` already says whether a URL means one video or a playlist is stated rather than
//! guessed. This is that sentence one level finer: the run says what a line means when the line does
//! not, and a line may say. What it costs is that a marked list is several yt-dlp runs rather than
//! one — see [`crate::fetch`], where the splitting happens.
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
//! # A file with no markers is not this tool's file
//!
//! It is an ordinary yt-dlp batch file, and [`crate::fetch`] hands it over untouched rather than
//! reading and rewriting it. That is what keeps a `km-video-fetch.txt` somebody maintains by hand
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
/// The counterpart of [`write`], and the difference is the whole reason both exist. `write` is for a
/// list this tool will read again — the markers are the point of it. This one is for a list *yt-dlp*
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
        for line in text.lines() {
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
/// The rule is one sentence and covers every shape at once: **every component must be an ordinary
/// name.** That turns down `..`, a leading separator, a drive letter, a `C:relative` and a UNC
/// prefix without naming any of them, and it means the same thing on both platforms.
pub fn destination(root: &Path, said: &str) -> Result<PathBuf> {
    let said = said.trim();
    let ordinary = !said.is_empty()
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
