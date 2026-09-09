//! Listing folders, because a browser cannot hand a web page one.
//!
//! # Why the picking happens on this side
//!
//! **A file input gives a page the file's *contents*, never its location** — a security property of
//! the browser. There is no folder input at all. So a page that
//! needs a folder either asks somebody to type a path or does the listing itself, and typing a path
//! is what people are already doing by hand.
//!
//! This grants nothing new. The program is on loopback, it already writes files wherever it is
//! pointed, and it runs as whoever started it — listing directories is strictly less than it can
//! already do.
//!
//! **Deliberately not a native dialog.** That would mean a GUI toolkit on every platform for one
//! interaction, in a program whose whole user interface is otherwise a page.
//!
//! # It never fails
//!
//! A folder that cannot be read becomes a message beside a trail that is still navigable, never a
//! 500. Half the folders on a Windows machine refuse a listing — system directories, another user's
//! profile, a disconnected network drive — and meeting one must not end the browse.

use std::path::{Path, PathBuf};

/// One directory, as the page draws it.
#[derive(Debug, Clone)]
pub struct Listing {
    /// Where this is, absolute. Empty at the top of a Windows machine, where there is no one root.
    ///
    /// **A string rather than a `PathBuf`**, and every path a template touches is one: askama's
    /// `json` filter needs something serializable, and the templates would otherwise all carry a
    /// `.display()` whose only purpose is to satisfy the type system.
    pub at: String,
    /// How it reads, for the crumb bar.
    pub shown: String,
    /// The parent, where there is one to go up to.
    pub up: Option<String>,
    /// The folders inside it, sorted.
    pub folders: Vec<Folder>,
    /// Why there is nothing to show, where that is the reason.
    pub error: Option<String>,
    /// Whether this is the drive list rather than a directory.
    pub roots: bool,
}

/// One row of a listing.
#[derive(Debug, Clone)]
pub struct Folder {
    /// Its own name.
    pub name: String,
    /// The whole path, which is what a click sends back.
    pub path: String,
}

/// Where a browse starts when nothing says otherwise.
///
/// The folder already chosen, if it still exists, else the home directory. Starting at the drive
/// list every time would make the common case — *the folder next to the one I used last* — four
/// clicks away.
#[must_use]
pub fn start(chosen: Option<&Path>) -> PathBuf {
    chosen
        .filter(|path| path.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .unwrap_or_default()
}

/// Lists one directory, or the drives when `at` is empty.
#[must_use]
pub fn list(at: &Path) -> Listing {
    if at.as_os_str().is_empty() {
        return roots();
    }

    let mut listing = Listing {
        at: at.display().to_string(),
        shown: at.display().to_string(),
        up: parent_of(at),
        folders: Vec::new(),
        error: None,
        roots: false,
    };

    let entries = match std::fs::read_dir(at) {
        Ok(entries) => entries,
        Err(error) => {
            listing.error = Some(format!("cannot read this folder: {error}"));
            return listing;
        }
    };

    for entry in entries.flatten() {
        // `file_type` rather than `metadata`, so a symlink is not followed. A link into a
        // disconnected network share otherwise stalls a listing for its whole timeout.
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Dot-directories are configuration and version control, not places anybody fetches into.
        if name.starts_with('.') {
            continue;
        }
        listing.folders.push(Folder {
            name,
            path: entry.path().display().to_string(),
        });
    }

    listing
        .folders
        .sort_by_key(|folder| folder.name.to_lowercase());
    listing
}

/// The drives, on Windows, and `/` everywhere else.
fn roots() -> Listing {
    let mut folders = Vec::new();

    if cfg!(windows) {
        // **Probed rather than enumerated through an API**, which keeps this a plain `std` program.
        // A letter that is not there fails `metadata` in microseconds; one that is a disconnected
        // network drive can take longer, which is the one cost of doing it this way.
        for letter in 'A'..='Z' {
            let root = PathBuf::from(format!("{letter}:\\"));
            if std::fs::metadata(&root).is_ok() {
                folders.push(Folder {
                    name: format!("{letter}:"),
                    path: root.display().to_string(),
                });
            }
        }
    } else {
        folders.push(Folder {
            name: "/".to_owned(),
            path: "/".to_owned(),
        });
    }

    Listing {
        at: String::new(),
        shown: if cfg!(windows) {
            "This computer".to_owned()
        } else {
            "/".to_owned()
        },
        up: None,
        folders,
        error: None,
        roots: true,
    }
}

/// The folder above this one, or the drive list at the top of a tree.
///
/// **`Some(empty)` rather than `None` at a drive root**, so Windows can climb from `D:\` to the list
/// of drives. Off Windows the top is `/` and there is genuinely nowhere above it.
fn parent_of(at: &Path) -> Option<String> {
    match at.parent() {
        Some(parent) => Some(parent.display().to_string()),
        None if cfg!(windows) => Some(String::new()),
        None => None,
    }
}

/// Tidies a path somebody typed.
///
/// Trailing separators and surrounding whitespace both arrive routinely — the first from a file
/// manager's address bar, the second from a paste — and neither should be the difference between a
/// folder being found and not.
#[must_use]
pub fn tidy(typed: &str) -> PathBuf {
    let trimmed = typed.trim().trim_matches('"');
    let without_trailing = trimmed.trim_end_matches(['/', '\\']);
    // ...except where trimming leaves nothing, which is what `C:\` and `/` become.
    if without_trailing.is_empty() || without_trailing.ends_with(':') {
        return PathBuf::from(trimmed);
    }
    PathBuf::from(without_trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_path_survives_a_paste() {
        assert_eq!(
            tidy("  D:/tunes/karaoke  "),
            PathBuf::from("D:/tunes/karaoke")
        );
        assert_eq!(
            tidy("D:\\tunes\\karaoke\\"),
            PathBuf::from("D:\\tunes\\karaoke")
        );
        assert_eq!(
            tidy("\"D:\\tunes\\karaoke\""),
            PathBuf::from("D:\\tunes\\karaoke"),
            "a Windows Explorer 'copy as path' brings its own quotes"
        );
    }

    /// A drive root trims to nothing useful, so it is left alone.
    #[test]
    fn a_root_is_not_trimmed_away() {
        assert_eq!(tidy("C:\\"), PathBuf::from("C:\\"));
        assert_eq!(tidy("/"), PathBuf::from("/"));
    }

    #[test]
    fn the_top_of_the_tree_lists_somewhere_to_go() {
        let listing = list(Path::new(""));
        assert!(listing.roots);
        assert!(listing.up.is_none(), "there is nothing above the top");
        assert!(!listing.folders.is_empty(), "a computer has at least one");
    }

    /// The failure that must not be a failure: half the folders on a Windows machine refuse.
    #[test]
    fn an_unreadable_folder_is_a_message_and_not_a_panic() {
        let listing = list(Path::new("/definitely/not/a/folder/anywhere"));
        assert!(listing.error.is_some(), "it says why");
        assert!(listing.folders.is_empty());
        assert!(listing.up.is_some(), "and the way back still works");
    }

    #[test]
    fn a_listing_is_folders_only_and_sorted() {
        let dir = std::env::temp_dir().join("km-video-downloader-browse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Zebra")).expect("a folder");
        std::fs::create_dir_all(dir.join("apple")).expect("a folder");
        std::fs::create_dir_all(dir.join(".hidden")).expect("a folder");
        std::fs::write(dir.join("a-file.txt"), "x").expect("a file");

        let listing = list(&dir);
        let names: Vec<_> = listing.folders.iter().map(|f| f.name.clone()).collect();
        assert_eq!(names, vec!["apple".to_owned(), "Zebra".to_owned()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
