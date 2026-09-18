# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each version links to its release, and that is where the installers live along with the install
steps for each platform and the SHA-256 of every artifact.

## [1.12.0] - 2026-09-18

### Added

- `--convert` takes a video already on disk and puts it in the shape a download arrives in. Name a
  file, or a folder to take every video in it, and repeat the option for more than one. The result
  goes into `--out` under the same name with a `.mp4` extension, the file named is read and never
  written, and a name that is in the output folder already is skipped.
- A video already in the shape packaging wants is copied rather than re-encoded.
- A Convert page in the window beside Fetch, over the same four parts. Its picker lists the video
  files in a folder as well as the folders.
- `--video` applies to a conversion, where a picture larger than the size asked for is re-encoded
  down to it, because re-encoding is what was asked for.
- Pushing a `v*` tag builds the Windows installer and attaches it to a draft release.

## [1.11.0] - 2026-09-12

### Added

- `--video full|small|tiny` caps the download at 1080p, 720p or 480p. `full` is what a run that
  names no size asks for, so an existing command line fetches what it always fetched.
- The same three sizes in the window, under *How*.
- `--video SIZE` as a list header setting, so one `.kmvf` can set the size for a whole folder of
  songs.

### Changed

- A re-encode copies AAC audio through untouched instead of rebuilding it, so a file that arrives
  with the audio format a karaoke package wants keeps the sound it arrived with. Audio in any other
  format is still converted to AAC at 192k.
- A re-encode leaves a multichannel source at its own channel count, the packaging profile having no
  opinion about channels.
- With `--normalize`, a video larger than the size asked for is re-encoded down to it. Without it,
  such a video is reported and left alone.

## [1.10.0] - 2026-09-11

### Added

- `task dist:setup:notarized` builds a macOS package signed with a Developer ID, notarized by Apple
  and stapled, so it opens on a first double-click, and proves all three before calling the build
  done.

### Changed

- Neither program changed, so for a Windows user this release is 1.9.0 under a new number.
- `task dist:setup` still produces an unsigned macOS package, which is the right file for a build
  being tested and the wrong one to hand anybody.

## [1.9.0] - 2026-09-08

### Added

- `task clean:old` removes staged folders from earlier versions.

### Changed

- `--help` for both programs, and the two macOS installer panes, rewritten in plain application
  language.

### Fixed

- Both setup programs shipped a build other than the one they named. Each searched for its staged
  folder and took the first match, which sorted to the older one, then read the version out of those
  same executables, so the label was right and the programs inside were not. The folder is named
  rather than searched for.
- Five labels in the window stated something untrue, among them a checkbox that re-fetches rather
  than overwrites, and a limit that applies per playlist rather than per run.

## [1.8.0] - 2026-09-07

### Added

- `.kmvf` as the extension for a list of links. It carries the program's icon and opens the
  downloader on a double-click, handing the list to a window that is already open rather than
  starting a second one.
- Per-line markers in a list: `--playlist`, `--no-playlist` and `--out <folder>` in front of a link,
  so one list can mix single videos with whole playlists and sort what arrives into folders, each
  keeping its own record of what it already holds.
- A list header, the lines above the first link, applying `--playlist`, `--subs`, `--normalize`,
  `--no-archive`, `--limit`, `--cookies-from-browser`, `--format` and `--sort` to all of it. What is
  passed on the command line wins.
- `task dist:setup` builds an Inno Setup installer on Windows and a package on macOS, each with a
  checkbox per program, and each testing its own install and uninstall on every build.
- `task dist:bin` stages one folder holding every program.

### Fixed

- A second launch of the downloader exited without saying anything.
- A dry run drew its rows as faults rather than as what it would fetch.
- A destination folder meant different things on Windows and macOS.
- The list a page writes for itself was left behind after a run.
- The window's child processes opened console windows of their own.

## [1.7.0] - 2026-09-07

The first release from this repository.

### Added

- `km-video-fetch`, the command line: download a video with yt-dlp as H.264 and AAC in MP4, write
  the title and artist into its tags, and report whether what arrived can be played.
- `km-video-downloader`, the same fetch with a window and an icon of its own.

### Changed

- The fetch came out of the karaoke app, so that nothing which plays songs also downloads them.
- ffmpeg is no longer linked. It and yt-dlp are run as programs rather than built against, which is
  what makes this a plain Rust build with no C toolchain.
- The fetch is a library function reported as events rather than a program that prints, which is
  what lets one sequence answer for both programs.

[1.12.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.12.0
[1.11.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.11.0
[1.10.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.10.0
[1.9.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.9.0
[1.8.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.8.0
[1.7.0]: https://github.com/rrgmc/km-video-tools/releases/tag/v1.7.0
