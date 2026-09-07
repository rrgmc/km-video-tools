//! Whether what arrived is what packaging wants, and putting it right when it is not.
//!
//! Glue over [`crate::probe`] and [`crate::profile`], and it is worth its own module because the
//! sequence matters: probe, then measure, then — only if asked — re-encode through a temporary name.
//!
//! **This used to be optional and no longer is.** In the karaoke app the module sat behind a
//! default-off `video` feature, because reaching the profile meant linking ffmpeg; a build without
//! the feature downloaded exactly as well and simply could not say whether what landed was playable.
//! Reading a file through `ffprobe` costs no linkage, so the feature is gone and the answer is
//! always given.
//!
//! Nothing here restates what the profile is — [`crate::profile`] is where that lives, and it in
//! turn is a copy of the karaoke app's, which is the authoritative one.

use std::path::{Path, PathBuf};

/// What one downloaded file turned out to be.
#[derive(Debug, Clone)]
pub struct Report {
    /// The file this describes.
    pub path: PathBuf,
    /// How it falls outside the profile. Empty means packaging will copy its bytes.
    pub mismatches: Vec<crate::profile::Mismatch>,
    /// What the container said about itself.
    pub info: crate::probe::VideoInfo,
}

impl Report {
    /// Whether packaging would store this file untouched.
    #[must_use]
    pub fn in_profile(&self) -> bool {
        self.mismatches.is_empty()
    }

    /// Whether the machine could not play it at all.
    #[must_use]
    pub fn blocking(&self) -> bool {
        crate::profile::is_blocking(&self.mismatches)
    }
}

/// Probes one file and measures it against the packaging profile.
pub fn inspect(path: &Path) -> anyhow::Result<Report> {
    use anyhow::Context;

    let info = crate::probe::probe(path).with_context(|| format!("probing {}", path.display()))?;
    let mismatches = crate::profile::DEFAULT.check(&info, path);
    Ok(Report {
        path: path.to_path_buf(),
        mismatches,
        info,
    })
}

/// Re-encodes a file into the profile, replacing it.
///
/// Through a temporary name and a rename, as packaging itself does, so an interrupted encode cannot
/// leave a half-written video where a whole one was. The source is removed only once the
/// replacement is in place.
pub fn normalize(
    report: &Report,
    encoders: &crate::profile::Encoders,
    on_progress: impl FnMut(crate::profile::Progress),
) -> anyhow::Result<PathBuf> {
    use anyhow::Context;

    let container = crate::profile::DEFAULT.container;
    let partial = report.path.with_extension(format!("{container}.part"));
    let destination = report.path.with_extension(container);

    crate::profile::transcode(
        &report.path,
        &partial,
        &crate::profile::DEFAULT,
        &report.info,
        encoders,
        on_progress,
    )
    .with_context(|| format!("re-encoding {}", report.path.display()))?;

    // The source goes first when the destination would overwrite it, because a rename onto a file
    // that is also the encode's input is the one ordering that can lose both.
    if destination != report.path {
        std::fs::remove_file(&report.path)
            .with_context(|| format!("removing {}", report.path.display()))?;
    }
    std::fs::rename(&partial, &destination)
        .with_context(|| format!("renaming {} into place", partial.display()))?;
    Ok(destination)
}
