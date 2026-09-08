//! What the page remembers between one run and the next.
//!
//! # Why anything is remembered at all
//!
//! Because the output folder is the one answer that does not change. Somebody curating a corpus
//! fetches into the same place for months, so asking again on every run is friction that buys
//! nothing. The options come with it for the same reason — a person who always normalizes should not
//! re-tick it each session.
//!
//! # It never fails
//!
//! [`load`] returns the defaults and [`save`] returns nothing, following `km-admin`'s `chosen.rs`
//! exactly. Missing, unreadable, blank, or written by a later build all mean the same thing —
//! *nothing remembered yet* — and none of them is worth a message, let alone a refusal to start.
//! Losing a remembered folder costs one paste; refusing to start over a settings file costs the
//! whole program.
//!
//! Writes go through a temporary file and a rename, so a machine that loses power mid-write keeps
//! the previous answer rather than half of the new one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file's own format version.
///
/// **Read as a gate, not as a migration.** A record from a later build is treated as absent, which
/// is why the field exists at all: a future version that adds a field somebody's older copy cannot
/// understand should reset to defaults, not half-load.
const VERSION: u32 = 1;

/// The name in the config directory.
const FILE: &str = "settings.json";

/// Everything the page puts back the way it found it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The format version. See [`VERSION`].
    #[serde(rename = "v", default)]
    pub version: u32,
    /// Where the files go. Empty until somebody sets one.
    #[serde(default)]
    pub out: String,
    /// Expand a playlist rather than taking the one video from its URL.
    #[serde(default)]
    pub playlist: bool,
    /// Re-encode anything that lands outside the profile.
    #[serde(default)]
    pub normalize: bool,
    /// Mux subtitles in.
    #[serde(default)]
    pub subs: bool,
    /// At most this many items from a playlist.
    #[serde(default)]
    pub limit: Option<u32>,
    /// A browser to take cookies from.
    #[serde(default)]
    pub cookies_from_browser: Option<String>,
}

impl Settings {
    /// The remembered folder as a path, where one has been set.
    #[must_use]
    pub fn out(&self) -> Option<PathBuf> {
        let trimmed = self.out.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    }
}

/// Where the file lives, given the data directory.
#[must_use]
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// What was remembered, or the defaults.
#[must_use]
pub fn load(data_dir: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(path(data_dir)) else {
        return Settings::default();
    };
    match serde_json::from_str::<Settings>(&text) {
        // A record from a later build is a record this copy cannot be sure it understands.
        Ok(settings) if settings.version <= VERSION => settings,
        _ => Settings::default(),
    }
}

/// Writes the settings down, and says nothing if it cannot.
pub fn save(data_dir: &Path, settings: &Settings) {
    let mut settings = settings.clone();
    settings.version = VERSION;

    let Ok(text) = serde_json::to_string_pretty(&settings) else {
        return;
    };
    let _ = std::fs::create_dir_all(data_dir);

    // Through a temporary name, so an interrupted write leaves the previous answer rather than a
    // truncated file that then reads as "nothing remembered".
    let final_path = path(data_dir);
    let partial = final_path.with_extension("json.part");
    if std::fs::write(&partial, text).is_ok() && std::fs::rename(&partial, &final_path).is_err() {
        // A rename can fail across some filesystems where a copy would not. Do not leave the
        // temporary file behind to be mistaken for something.
        let _ = std::fs::remove_file(&partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("km-video-downloader-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    #[test]
    fn what_was_written_comes_back() {
        let dir = scratch("roundtrip");
        let mut settings = Settings {
            out: "D:/tunes/karaoke".to_owned(),
            normalize: true,
            limit: Some(25),
            cookies_from_browser: Some("firefox".to_owned()),
            ..Settings::default()
        };
        save(&dir, &settings);

        settings.version = VERSION;
        assert_eq!(load(&dir), settings);
        assert_eq!(
            load(&dir).out(),
            Some(PathBuf::from("D:/tunes/karaoke")),
            "a Windows path survives the round trip"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The four ways there is nothing to restore, and every one of them is ordinary.
    #[test]
    fn anything_unreadable_is_simply_nothing_remembered() {
        let dir = scratch("unreadable");

        assert_eq!(load(&dir), Settings::default(), "no file yet");

        std::fs::write(path(&dir), "").expect("write");
        assert_eq!(load(&dir), Settings::default(), "a blank file");

        std::fs::write(path(&dir), "{not json at all").expect("write");
        assert_eq!(load(&dir), Settings::default(), "a broken file");

        std::fs::write(path(&dir), r#"{"v":99,"out":"D:/x"}"#).expect("write");
        assert_eq!(
            load(&dir),
            Settings::default(),
            "a record from a later build is not half-read"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A blank folder is no folder, so the page asks for one rather than fetching into `""`.
    #[test]
    fn a_blank_folder_is_no_folder() {
        assert_eq!(Settings::default().out(), None);
        assert_eq!(
            Settings {
                out: "   ".to_owned(),
                ..Settings::default()
            }
            .out(),
            None
        );
    }

    #[test]
    fn a_write_leaves_no_temporary_file_behind() {
        let dir = scratch("temp");
        save(&dir, &Settings::default());
        let left: Vec<_> = std::fs::read_dir(&dir)
            .expect("read the directory")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec![FILE.to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
