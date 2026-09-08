//! What a video file says about itself, read by asking `ffprobe`.
//!
//! # Why a subprocess rather than a library
//!
//! The karaoke app reads the same facts through `ffmpeg-next`, in `crates/playback/km-video`, and
//! that is right there: it is already decoding the pictures, so the container is open anyway.
//!
//! Here it would be a bad trade. `km-video` is `#![cfg(feature = "ffmpeg")]` at crate level over an
//! unconditional dependency on the machine's audio crate, so it cannot be sliced thinly — borrowing
//! [`VideoInfo`] and [`supports_pixel_format`] from it would mean linking ffmpeg, requiring libclang
//! at build time for bindgen, and compiling a synthesizer and an audio host, in a program whose
//! entire job is to run two other programs.
//!
//! And the subprocess costs nothing that is not already spent. **ffmpeg is a hard requirement of
//! this tool either way**: yt-dlp muxes the video and audio it downloads with it, and
//! [`crate::profile::transcode`] shells out to it to re-encode. `ffprobe` ships in the same package
//! as `ffmpeg`; a machine that can run one can run the other.
//!
//! So this is a second implementation of the same reading, deliberately, and the fields below are
//! copied from `km-video` so the two produce the same struct. [`crate::profile`] carries the same
//! note for the same reason.
//!
//! # Two things ffmpeg's own API gives away that this has to do by hand
//!
//! * **Which stream is the picture.** `input.streams().best(Type::Video)` is one call; here every
//!   stream comes back in a list and cover art *is* a video stream. See [`video_stream`].
//! * **Tag case.** `ffmpeg_next` normalizes each container's own spelling into lowercase keys, so
//!   MP4's `©nam` and Matroska's `TITLE` both arrive as `title`. `ffprobe` reports the container's
//!   own case, so tags are looked up case-insensitively. See [`tag`].

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::child::without_a_console_window;

/// Which ffprobe binary to run.
///
/// Whatever is on `PATH`, matching [`crate::profile`]'s treatment of `ffmpeg` and for the same
/// reason: the machine doing the downloading already has one, and shelling out to it is what keeps
/// this program a plain Rust binary that links nothing.
const FFPROBE: &str = "ffprobe";

/// What a video file says about itself, read without decoding it.
///
/// This is what packaging records, so the machine reads a fact rather than opening a file to find
/// out how long a song is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoInfo {
    /// Length in milliseconds.
    pub duration_ms: u32,
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Frames per second, times 1000, so the common 29.97 survives being written down.
    pub frame_rate_milli: u32,
    /// The audio stream's sample rate.
    pub audio_sample_rate: u32,
    /// The video codec, spelled as ffmpeg spells it: `h264`, `vp9`, `av1`.
    pub video_codec: String,
    /// The audio codec, spelled the same way: `aac`, `opus`, `mp3`.
    pub audio_codec: String,
    /// How the picture is laid out: `yuv420p`, `yuv444p`, `yuv420p10le`.
    ///
    /// The one field here that decides whether the file can be *played* rather than merely how well
    /// it is suited — see [`supports_pixel_format`].
    pub pixel_format: String,
    /// Audio channels in the file, before conversion.
    ///
    /// Reported for packaging's benefit rather than the player's: the machine's decoder runs a
    /// `swresample` context that converts whatever the file has to interleaved stereo `f32`, so
    /// mono, 5.1 and anything else all play. Contrast the pixel format, which nothing converts.
    pub audio_channels: u16,
    /// The container's own title tag, if it carries one.
    ///
    /// Read here rather than guessed from the file name, because a downloader that knows the song's
    /// real title can write it down and a file name cannot always carry it — a title with a `/` or
    /// a `:` in it survives in a tag and does not survive in a path.
    pub title: Option<String>,
    /// The container's own artist tag, if it carries one.
    ///
    /// The one fact a video song has never had. A MIDI file's artist comes out of its lyrics header;
    /// a video's has to come from whoever downloaded it, and this is where they put it.
    pub artist: Option<String>,
}

/// Whether a picture in this format can be drawn by the machine.
///
/// True for exactly the 8-bit planar 4:2:0 layouts. `yuvj420p` is the deprecated full-range spelling
/// of `yuv420p` — the same three planes at the same sizes, differing only in how the values are
/// interpreted, which the GPU's conversion handles — so refusing it would reject a large number of
/// perfectly ordinary files for no reason anything here can act on.
#[must_use]
pub fn supports_pixel_format(format: &str) -> bool {
    matches!(format, "yuv420p" | "yuvj420p")
}

/// Reads one file's shape by running `ffprobe` over it.
///
/// Fails when ffprobe is missing, when it refuses the file, or when the file has no video or no
/// audio stream — the last two being the same refusals `km-video` makes, because a karaoke video
/// with no sound and a sound file with no picture are both things the machine cannot use.
pub fn probe(path: &Path) -> Result<VideoInfo> {
    let output = without_a_console_window(&mut Command::new(FFPROBE))
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!("could not run `{FFPROBE}` — checking a download needs ffmpeg on the PATH")
        })?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        if detail.is_empty() {
            bail!("`{FFPROBE}` exited with {}", output.status);
        }
        bail!("{detail}");
    }

    let text = String::from_utf8_lossy(&output.stdout);
    info_of(&text, path)
}

/// Reads the shape out of one `ffprobe -print_format json` document.
///
/// Split from [`probe`] so the parsing is testable without a file on disk or ffmpeg installed, on
/// the captured documents the tests below carry. `path` is used only in messages.
fn info_of(json: &str, path: &Path) -> Result<VideoInfo> {
    let probed: Probed =
        serde_json::from_str(json).with_context(|| format!("reading {}", path.display()))?;

    let Some(video) = video_stream(&probed.streams) else {
        bail!("{} has no video stream", path.display());
    };
    let Some(audio) = probed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
    else {
        bail!("{} has no audio stream", path.display());
    };

    Ok(VideoInfo {
        duration_ms: probed.format.duration.as_deref().map_or(0, duration_ms),
        width: video.width.unwrap_or(0),
        height: video.height.unwrap_or(0),
        frame_rate_milli: video.avg_frame_rate.as_deref().map_or(0, frame_rate_milli),
        audio_sample_rate: audio
            .sample_rate
            .as_deref()
            .and_then(|rate| rate.parse().ok())
            .unwrap_or(0),
        video_codec: video.codec_name.clone().unwrap_or_default(),
        audio_codec: audio.codec_name.clone().unwrap_or_default(),
        pixel_format: video.pix_fmt.clone().unwrap_or_default(),
        audio_channels: audio.channels.unwrap_or(0),
        title: tag(&probed.format.tags, &["title"]),
        artist: tag(&probed.format.tags, &["artist", "album_artist", "author"]),
    })
}

/// The picture stream, skipping cover art.
///
/// **`attached_pic` is why this is a function and not a `find` written inline.** In MP4 a thumbnail
/// is muxed as a second video stream, and reading *it* would describe a still JPEG: one frame, no
/// frame rate, and whatever pixel format the picture happened to be in — which would then be
/// measured against the profile and reported as a file the machine cannot play. It is the exact
/// hazard [`crate::args`] refuses `--embed-thumbnail` over, and a file fetched by other means can
/// still arrive carrying one.
fn video_stream(streams: &[Stream]) -> Option<&Stream> {
    streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video")
            && stream.disposition.attached_pic.unwrap_or(0) == 0
    })
}

/// `"30000/1001"` as 29970, and anything unreadable as zero.
///
/// The rational is kept rather than rounded because the ceiling it is compared against is written in
/// the same units — see [`crate::profile::DEFAULT`], where 29.97 passing a 30 fps test is the point.
fn frame_rate_milli(rate: &str) -> u32 {
    let Some((numerator, denominator)) = rate.split_once('/') else {
        return 0;
    };
    let (Ok(numerator), Ok(denominator)) = (numerator.parse::<i64>(), denominator.parse::<i64>())
    else {
        return 0;
    };
    if denominator <= 0 {
        return 0;
    }
    u32::try_from(numerator * 1000 / denominator).unwrap_or(0)
}

/// `"273.020000"` seconds as 273020 milliseconds.
///
/// ffprobe reports the format's duration as a decimal string of seconds, and `"N/A"` when the
/// container does not say — a live stream, or a fragment. Zero is the honest answer to that, and it
/// is what [`crate::profile::Progress::percent`] already guards against.
fn duration_ms(seconds: &str) -> u32 {
    let Ok(seconds) = seconds.parse::<f64>() else {
        return 0;
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let millis = (seconds * 1000.0).round();
    if millis >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    // Checked above: finite, positive, and below u32::MAX.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounds are checked immediately above"
    )]
    {
        millis as u32
    }
}

/// The first of `keys` the container actually carries, trimmed, ignoring blanks.
///
/// Two things at once, and both are load-bearing:
///
/// *Case.* Matroska spells its tags `TITLE` and `ARTIST`, MP4 uses `©nam`/`©ART` which ffprobe
/// reports as `title`/`artist`, and an ID3 tag arrives in yet another spelling. `ffmpeg_next`
/// normalizes all of them; ffprobe does not, so the comparison is case-insensitive here instead.
///
/// *Blank rather than absent.* A muxer asked to embed metadata it does not have writes the key with
/// an empty value, and `Some("")` propagated onwards would become a song titled nothing at all.
fn tag(tags: &std::collections::BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| {
            tags.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The shape of `ffprobe -print_format json`, as far as anything here reads it.
#[derive(Debug, Deserialize)]
struct Probed {
    #[serde(default)]
    streams: Vec<Stream>,
    #[serde(default)]
    format: Format,
}

/// One stream out of the container.
#[derive(Debug, Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    avg_frame_rate: Option<String>,
    /// A string in ffprobe's output, not a number — `"44100"`.
    sample_rate: Option<String>,
    channels: Option<u16>,
    #[serde(default)]
    disposition: Disposition,
}

/// The flags ffprobe reports per stream. Only one of them matters here.
#[derive(Debug, Default, Deserialize)]
struct Disposition {
    attached_pic: Option<u8>,
}

/// The container-level half of the document.
#[derive(Debug, Default, Deserialize)]
struct Format {
    /// Seconds as a decimal string, or absent.
    duration: Option<String>,
    #[serde(default)]
    tags: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A yt-dlp download of the shape this tool aims for: H.264 in yuv420p at 29.97, AAC stereo,
    /// MP4, with the title and artist this tool wrote into it.
    const A_REAL_DOWNLOAD: &str = r#"{
        "streams": [
            {
                "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                "pix_fmt": "yuv420p", "avg_frame_rate": "30000/1001",
                "disposition": {"attached_pic": 0}
            },
            {
                "codec_type": "audio", "codec_name": "aac", "sample_rate": "44100", "channels": 2,
                "avg_frame_rate": "0/0", "disposition": {"attached_pic": 0}
            }
        ],
        "format": {
            "duration": "273.020000",
            "tags": {"title": "Aeng Moo Sae", "artist": "Howl"}
        }
    }"#;

    #[test]
    fn a_real_download_reads_the_way_the_profile_expects() {
        let info = info_of(A_REAL_DOWNLOAD, Path::new("a.mp4")).expect("parses");
        assert_eq!(info.duration_ms, 273_020);
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.audio_sample_rate, 44_100);
        assert_eq!(info.audio_channels, 2);
        assert_eq!(info.video_codec, "h264");
        assert_eq!(info.audio_codec, "aac");
        assert_eq!(info.pixel_format, "yuv420p");
        assert_eq!(info.title.as_deref(), Some("Aeng Moo Sae"));
        assert_eq!(info.artist.as_deref(), Some("Howl"));

        // The whole point of the milli-fps representation, asserted where it is produced rather
        // than only where it is compared.
        assert_eq!(info.frame_rate_milli, 29_970);
        assert!(
            crate::profile::DEFAULT
                .check(&info, Path::new("a.mp4"))
                .is_empty()
        );
    }

    /// The hazard this parser exists to avoid: cover art is a video stream, and it is often first.
    #[test]
    fn cover_art_is_not_mistaken_for_the_picture() {
        let json = r#"{
            "streams": [
                {
                    "codec_type": "video", "codec_name": "mjpeg", "width": 640, "height": 640,
                    "pix_fmt": "yuvj444p", "avg_frame_rate": "90000/3003",
                    "disposition": {"attached_pic": 1}
                },
                {
                    "codec_type": "video", "codec_name": "h264", "width": 1280, "height": 720,
                    "pix_fmt": "yuv420p", "avg_frame_rate": "25/1",
                    "disposition": {"attached_pic": 0}
                },
                {
                    "codec_type": "audio", "codec_name": "aac", "sample_rate": "48000",
                    "channels": 2, "disposition": {"attached_pic": 0}
                }
            ],
            "format": {"duration": "10.0", "tags": {}}
        }"#;
        let info = info_of(json, Path::new("a.mp4")).expect("parses");
        assert_eq!(info.video_codec, "h264", "not the JPEG");
        assert_eq!(info.width, 1280);
        assert_eq!(info.pixel_format, "yuv420p");
        assert!(
            crate::profile::DEFAULT
                .check(&info, Path::new("a.mp4"))
                .is_empty(),
            "reading the cover art instead would have called this unplayable"
        );
    }

    /// Matroska writes its tags in upper case and ffprobe does not normalize them.
    #[test]
    fn tags_are_found_whatever_case_the_container_wrote_them_in() {
        let json = r#"{
            "streams": [
                {
                    "codec_type": "video", "codec_name": "vp9", "width": 1920, "height": 1080,
                    "pix_fmt": "yuv420p", "avg_frame_rate": "30/1"
                },
                {"codec_type": "audio", "codec_name": "opus", "sample_rate": "48000", "channels": 2}
            ],
            "format": {
                "duration": "200.5",
                "tags": {"TITLE": "  Sultans of Swing  ", "ALBUM_ARTIST": "Dire Straits"}
            }
        }"#;
        let info = info_of(json, Path::new("a.mkv")).expect("parses");
        assert_eq!(info.title.as_deref(), Some("Sultans of Swing"), "trimmed");
        assert_eq!(
            info.artist.as_deref(),
            Some("Dire Straits"),
            "album_artist is consulted after artist, and case does not matter"
        );
        assert_eq!(info.duration_ms, 200_500);
    }

    /// A muxer with no metadata to embed writes the keys anyway, with nothing in them.
    #[test]
    fn a_blank_tag_is_no_tag() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "codec_name": "h264", "width": 640, "height": 480,
                 "pix_fmt": "yuv420p", "avg_frame_rate": "25/1"},
                {"codec_type": "audio", "codec_name": "aac", "sample_rate": "44100", "channels": 1}
            ],
            "format": {"duration": "N/A", "tags": {"title": "   ", "artist": ""}}
        }"#;
        let info = info_of(json, Path::new("a.mp4")).expect("parses");
        assert_eq!(info.title, None);
        assert_eq!(info.artist, None);
        assert_eq!(
            info.duration_ms, 0,
            "an unreadable duration is zero, not a failure"
        );
    }

    #[test]
    fn a_file_with_no_sound_is_refused() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "codec_name": "h264", "width": 640, "height": 480,
                 "pix_fmt": "yuv420p", "avg_frame_rate": "25/1"}
            ],
            "format": {"duration": "10.0"}
        }"#;
        let error = info_of(json, Path::new("silent.mp4")).expect_err("no audio stream");
        assert!(error.to_string().contains("no audio stream"), "{error}");
    }

    #[test]
    fn a_file_that_is_only_cover_art_has_no_picture() {
        let json = r#"{
            "streams": [
                {"codec_type": "video", "codec_name": "mjpeg", "width": 640, "height": 640,
                 "pix_fmt": "yuvj420p", "disposition": {"attached_pic": 1}},
                {"codec_type": "audio", "codec_name": "mp3", "sample_rate": "44100", "channels": 2}
            ],
            "format": {"duration": "180.0"}
        }"#;
        let error = info_of(json, Path::new("song.mp3")).expect_err("no usable video stream");
        assert!(error.to_string().contains("no video stream"), "{error}");
    }

    #[test]
    fn an_unreadable_frame_rate_is_zero_rather_than_a_panic() {
        assert_eq!(frame_rate_milli("30000/1001"), 29_970);
        assert_eq!(frame_rate_milli("25/1"), 25_000);
        assert_eq!(frame_rate_milli("0/0"), 0, "no denominator");
        assert_eq!(frame_rate_milli("30"), 0, "not a rational at all");
        assert_eq!(frame_rate_milli(""), 0);
    }

    #[test]
    fn durations_round_to_the_nearest_millisecond() {
        assert_eq!(duration_ms("273.020000"), 273_020);
        assert_eq!(duration_ms("0.0005"), 1, "rounded, not truncated");
        assert_eq!(duration_ms("N/A"), 0);
        assert_eq!(duration_ms("-1"), 0);
    }
}
