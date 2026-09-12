//! How much picture to ask for, and what a re-encode at that size aims at.
//!
//! # Why the size is a choice and the profile is not
//!
//! [`crate::profile::DEFAULT`] states what the appliance will accept, and it is a constant because
//! the point of normalizing is that there is one answer. What this module holds is a different
//! question with a different owner: how much of the allowed picture somebody wants to spend disk on.
//!
//! The two never conflict, because the profile's limits are ceilings. A 720p H.264/AAC MP4 is inside
//! `max_width: 1920` and `max_height: 1080` exactly as a 1080p one is, so every step here lands in
//! profile and packaging copies its bytes.
//!
//! # Why it is worth asking for
//!
//! A karaoke video is a caption over a background, and the song is the audio. Half the bytes for a
//! picture nobody is studying is a good trade on a machine holding thousands of songs, and it is one
//! only the person filling the disk can make.
//!
//! # The cheap half and the expensive half
//!
//! A step says two things, and they are reached by different means.
//!
//! **What to ask a site for.** yt-dlp selects the video and the audio stream separately and muxes
//! them, so a height cap changes the picture and nothing else: the audio comes from `ba` and reaches
//! the file untouched. This costs no CPU and loses nothing beyond the resolution itself, which is why
//! it is the half that matters. See [`crate::args::format_for`].
//!
//! **What to re-encode down to**, for a file that arrives larger because the site offered nothing
//! smaller. That costs minutes per song and a generation of picture quality, so it happens only where
//! `--normalize` asked for it. See [`Encode`] and [`crate::profile::transcode`].

use serde::{Deserialize, Serialize};

/// How much picture to ask for.
///
/// Named steps rather than a height and a quality number, because the two move together: a smaller
/// picture wants a higher constant rate factor to be worth encoding at all, and a person choosing
/// between them is choosing how much disk a song costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Video {
    /// 1080p, the largest picture packaging accepts.
    #[default]
    Full,
    /// 720p, about half the bytes.
    Small,
    /// 480p, about a quarter.
    Tiny,
}

impl std::fmt::Display for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

/// A word that names no step.
///
/// A type of its own so that clap, which this crate deliberately does not depend on, can take it as
/// a parse failure and word it for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownVideo(String);

impl std::fmt::Display for UnknownVideo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\"{}\" is not a video size. It is one of {}.",
            self.0,
            Video::WORDS.join(", ")
        )
    }
}

impl std::error::Error for UnknownVideo {}

impl std::str::FromStr for Video {
    type Err = UnknownVideo;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "small" => Ok(Self::Small),
            "tiny" => Ok(Self::Tiny),
            other => Err(UnknownVideo(other.to_owned())),
        }
    }
}

impl Video {
    /// Every step, in the order a person meets them: most picture first.
    pub const ALL: [Self; 3] = [Self::Full, Self::Small, Self::Tiny];

    /// The words [`Video::from_str`] accepts, for anything offering the choice.
    pub const WORDS: [&'static str; 3] = ["full", "small", "tiny"];

    /// This step's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Small => "small",
            Self::Tiny => "tiny",
        }
    }

    /// The tallest picture this step asks a site for, and re-encodes down to.
    ///
    /// The number a person recognises, because it is the one yt-dlp's format selector and every
    /// description of a video are both written in.
    #[must_use]
    pub fn max_height(self) -> u32 {
        match self {
            Self::Full => 1080,
            Self::Small => 720,
            Self::Tiny => 480,
        }
    }

    /// The matching width, which is that height at 16:9.
    ///
    /// A ceiling of its own rather than something derived while scaling, because an ultrawide source
    /// is bounded by its width and a 4:3 one by its height, and the scale filter needs both to keep
    /// either from slipping through. Each of these is even, which 4:2:0 requires.
    #[must_use]
    pub fn max_width(self) -> u32 {
        match self {
            Self::Full => 1920,
            Self::Small => 1280,
            Self::Tiny => 854,
        }
    }

    /// What a re-encode at this step aims at.
    ///
    /// The frame rate and the container come from the profile whatever the step, because neither is
    /// a matter of how much disk a picture costs: 30 fps is what a caption over a picture needs, and
    /// the container is what packaging stores.
    #[must_use]
    pub fn encode(self) -> Encode {
        Encode {
            max_width: self.max_width(),
            max_height: self.max_height(),
            max_frame_rate_milli: crate::profile::DEFAULT.max_frame_rate_milli,
            crf: match self {
                Self::Full => 20,
                Self::Small => 24,
                Self::Tiny => 26,
            },
            bitrate: match self {
                Self::Full => "4M",
                Self::Small => "2M",
                Self::Tiny => "1M",
            },
            container: crate::profile::DEFAULT.container,
        }
    }
}

/// What one re-encode aims at, as distinct from what packaging will accept.
///
/// Separate from [`crate::profile::Profile`] for two reasons. A profile is a test a finished file
/// passes or fails and carries nothing about how to reach it; and that struct is a copy of the
/// karaoke app's, which decides what a package may contain and has no interest in how much disk
/// somebody here wants to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encode {
    /// Widest picture written, as a ceiling: a narrower source is left at its own size.
    pub max_width: u32,
    /// Tallest picture written.
    pub max_height: u32,
    /// Fastest frame rate written, in the milli-fps [`crate::probe::VideoInfo`] reports.
    pub max_frame_rate_milli: u32,
    /// x264's constant rate factor. Lower is a better picture and a bigger file.
    pub crf: u8,
    /// What libopenh264 gets instead, having no constant rate factor of its own.
    ///
    /// An average bitrate chosen to look similar at the matching height, which is the nearest thing
    /// that encoder offers.
    pub bitrate: &'static str,
    /// The container, by file extension.
    pub container: &'static str,
}

impl Encode {
    /// How `info` overruns this target, worded for a person, or `None` where it does not.
    ///
    /// The form [`crate::profile::Mismatch`] uses — found, then wanted — so a caller joining this
    /// with the profile's findings reads one list in one voice.
    ///
    /// **The frame rate is not asked about.** Its ceiling is the profile's at every step, so a file
    /// that overruns it is already outside the profile and has been reported as such; repeating it
    /// here would say the same thing twice about one file.
    #[must_use]
    pub fn exceeded_by(&self, info: &crate::probe::VideoInfo) -> Option<String> {
        (info.width > self.max_width || info.height > self.max_height).then(|| {
            format!(
                "{}x{}, asked for at most {}x{}",
                info.width, info.height, self.max_width, self.max_height
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::VideoInfo;
    use crate::profile;

    /// A 1080p download with the sound a karaoke package wants, which is what nearly every fetch
    /// produces and therefore what every question here is asked about.
    fn arrived() -> VideoInfo {
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

    /// The question the whole choice turns on, and the negative half matters as much: a file inside
    /// the step it was fetched under is not something to spend an hour of CPU on.
    #[test]
    fn a_step_says_how_far_over_it_a_file_is() {
        assert_eq!(Video::Full.encode().exceeded_by(&arrived()), None);

        let over = Video::Small
            .encode()
            .exceeded_by(&arrived())
            .expect("1080p overruns 720p");
        assert!(over.contains("1920x1080"), "{over}");
        assert!(over.contains("1280x720"), "{over}");

        let mut smaller = arrived();
        smaller.width = 1280;
        smaller.height = 720;
        assert_eq!(Video::Small.encode().exceeded_by(&smaller), None);
        assert!(Video::Tiny.encode().exceeded_by(&smaller).is_some());
    }

    /// An ultrawide picture is bounded by its width, which is why the ceiling is a pair rather than
    /// a height alone.
    #[test]
    fn a_wide_short_picture_is_caught_by_its_width() {
        let mut wide = arrived();
        wide.width = 2560;
        wide.height = 640;
        assert!(Video::Small.encode().exceeded_by(&wide).is_some());
    }

    /// The default step is the whole of what a run with no size named asks for, so such a run gets
    /// the argv it would have got with no choice on offer at all.
    #[test]
    fn full_is_the_default_and_the_largest() {
        assert_eq!(Video::default(), Video::Full);

        let encode = Video::Full.encode();
        assert_eq!(encode.max_width, profile::DEFAULT.max_width);
        assert_eq!(encode.max_height, profile::DEFAULT.max_height);
        assert_eq!(
            encode.max_frame_rate_milli,
            profile::DEFAULT.max_frame_rate_milli
        );
        assert_eq!(encode.crf, 20);
        assert_eq!(encode.bitrate, "4M");
        assert_eq!(encode.container, "mp4");
    }

    /// The property that makes the whole choice free: whatever step somebody picks, the file it
    /// produces is one packaging copies rather than re-encodes.
    #[test]
    fn every_step_lands_inside_the_packaging_profile() {
        for step in Video::ALL {
            let encode = step.encode();
            assert!(
                encode.max_width <= profile::DEFAULT.max_width,
                "{step} is wider than packaging accepts"
            );
            assert!(
                encode.max_height <= profile::DEFAULT.max_height,
                "{step} is taller than packaging accepts"
            );
            assert!(
                encode.max_frame_rate_milli <= profile::DEFAULT.max_frame_rate_milli,
                "{step} is faster than packaging accepts"
            );
            assert_eq!(encode.container, profile::DEFAULT.container);
        }
    }

    /// Both ceilings are even, which is what 4:2:0 chroma at half width and half height needs.
    #[test]
    fn every_step_is_an_even_number_of_pixels_each_way() {
        for step in Video::ALL {
            assert_eq!(step.max_width() % 2, 0, "{step} has an odd width");
            assert_eq!(step.max_height() % 2, 0, "{step} has an odd height");
        }
    }

    /// A smaller step is smaller in every direction, which is what lets one word stand for the whole
    /// trade.
    #[test]
    fn the_steps_descend() {
        assert!(Video::Full.max_height() > Video::Small.max_height());
        assert!(Video::Small.max_height() > Video::Tiny.max_height());
        assert!(Video::Full.max_width() > Video::Small.max_width());
        assert!(Video::Small.max_width() > Video::Tiny.max_width());
        assert!(Video::Full.encode().crf < Video::Small.encode().crf);
        assert!(Video::Small.encode().crf < Video::Tiny.encode().crf);
    }

    /// One spelling for the command line, the form and a list header alike.
    #[test]
    fn a_step_reads_back_as_the_word_it_prints() {
        for step in Video::ALL {
            assert_eq!(step.to_string().parse(), Ok(step));
        }
        assert_eq!("  SMALL ".parse(), Ok(Video::Small));
        assert_eq!(Video::WORDS, Video::ALL.map(Video::word));
    }

    /// An unknown word names itself and the alternatives, because it reaches a person as a usage
    /// error from whichever surface they typed it into.
    #[test]
    fn an_unknown_word_says_what_it_knows() {
        let refused = "huge".parse::<Video>().expect_err("not a step");
        let said = refused.to_string();
        assert!(said.contains("huge"), "{said}");
        for word in Video::WORDS {
            assert!(said.contains(word), "{said}");
        }
    }

    /// The step travels through the settings file as its word, so a file somebody opens reads as
    /// what they chose.
    #[test]
    fn a_step_is_written_down_as_its_word() {
        let written = serde_json::to_string(&Video::Small).expect("serializes");
        assert_eq!(written, "\"small\"");
        assert_eq!(
            serde_json::from_str::<Video>("\"tiny\"").expect("reads back"),
            Video::Tiny
        );
    }
}
