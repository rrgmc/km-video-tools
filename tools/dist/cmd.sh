#!/usr/bin/env bash
#
# Stages one command-line program into a folder somebody can be handed.
#
#   tools/dist/cmd.sh                        # every program in this workspace
#   tools/dist/cmd.sh km-video-fetch         # just that one
#   tools/dist/cmd.sh --no-build             # stage what is already built
#   tools/dist/cmd.sh --zip                  # also produce a versioned .zip of each folder
#
#   dist/<app>/<platform>/<app>-<version>-<triple>/
#       <app>[.exe]
#       README.txt
#       LICENSE-MIT  LICENSE-APACHE
#
# **`ALL_APPS` is the list, and adding a member to the workspace does not add it here.** That is the
# intended shape: a library crate has nothing to stage, and a program that is not ready to hand over
# should not be handed over by accident. When the web UI this repository is laid out for exists, it
# goes in this list — and probably in a script of its own, because a program with a window has
# platform-specific staging that a command line does not.
#
# **The README is written here rather than committed.** It is the document a person reading the
# folder actually opens, it has to name the version and the platform they were given, and there is no
# second copy of it to drift.

set -euo pipefail

cd "$(dirname "$0")/../.."

. tools/dist/common.sh

ALL_APPS=(km-video-fetch)

# The document in the folder. Dispatched by name, so a program with no README here is a hard failure
# rather than a folder that quietly ships without one.
readme() { # <app> <version>
  case "$1" in
    km-video-fetch) readme_km_video_fetch "$2" ;;
    *) echo "dist: no README is written for $1" >&2; return 1 ;;
  esac
}

readme_km_video_fetch() { # <version>
  cat <<README
km-video-fetch $1
$(printf '=%.0s' $(seq 1 $((15 + ${#1}))))

Download video songs with yt-dlp, in the shape a karaoke package wants them, and say whether they
arrived that way.

Two things this does that a remembered yt-dlp command line does not. It asks for H.264 and AAC at no
more than 1080p30 in MP4 -- the shape a karaoke package stores video songs in -- so packaging copies
the file instead of spending an hour re-encoding it. And it asks yt-dlp to write the title and artist
into the file's own tags, which is where curation reads them from; without that a video song is
titled after its file name and has no artist until somebody types one in.

What you need first
-------------------

yt-dlp, and ffmpeg. Neither is included here and neither is downloaded for you.

    yt-dlp        https://github.com/yt-dlp/yt-dlp -- pipx, winget, brew or your package manager
    ffmpeg        yt-dlp needs it to join the video and audio streams and to write the tags.
                  ffprobe, from the same package, is what reads the file back afterwards

Keep yt-dlp current. YouTube changes what it serves and yt-dlp follows; a copy more than a few
months old fails in ways that look like a broken network. This tool prints the version it found and
says so when it is old.

Running it
----------

    km-video-fetch <url> --out ./songs

fetches one video. A URL that came from a playlist page fetches only that video -- add --playlist
to take the whole list, which is a thing you say rather than something guessed from the URL.

    km-video-fetch <playlist url> --playlist --out ./songs
    km-video-fetch --from-file urls.txt --out ./songs

Already-fetched videos are remembered in .km-fetched.txt beside them, so re-running over the same
list or playlist picks up only what is new. A folder can also carry its own list of what to fetch
into it, as km-video-fetch.txt, which is read when no URLs and no --from-file are given.

Options
-------

    --playlist                expand a playlist rather than taking one video from its URL
    -o, --out DIR             where the files go, default the current folder
    --limit N                 take at most N items from a playlist
    --from-file PATH          read URLs from a file, one per line
    --no-archive              fetch things that are already in the archive again
    --cookies-from-browser B  for material that needs an account (firefox, chrome, edge, ...)
    --subs                    mux subtitles in. Off by default; the machine does not read them
    --format SELECTOR         replace the format selector, if you know what you want instead
    --sort ORDER              replace the format sort order
    --normalize               re-encode anything that landed outside the profile
    --strict                  exit 2 if any file cannot be played as it is
    --dry-run                 list what would be fetched and fetch nothing
    --show-command            print the yt-dlp command line before running it
    --yt-dlp PATH             the yt-dlp to run, when it is not on the PATH
    --version

What it fetches, and from where, is your business
-------------------------------------------------

This tool runs yt-dlp against whatever you point it at. Downloading from a site may be contrary to
that site's terms, and the videos are somebody else's copyrighted work. That is a matter for whoever
runs the download.

Licenses
--------

km-video-fetch is MIT OR Apache-2.0, at your option; both texts are beside this file. yt-dlp and
ffmpeg are separate programs under their own licenses and are not included here.
README
}

BUILD=1
ZIP=0
APPS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --zip)      ZIP=1 ;;
    -h|--help)  sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)         echo "dist: unknown option $1" >&2; exit 2 ;;
    *)          APPS+=("$1") ;;
  esac
  shift
done

[ ${#APPS[@]} -gt 0 ] || APPS=("${ALL_APPS[@]}")

TRIPLE="$(dist_host_triple)"
PLATFORM="$(dist_platform "$TRIPLE")"
EXT="$(dist_exe_ext "$TRIPLE")"

if [ "$BUILD" = 1 ]; then
  echo "dist: building ${APPS[*]} in release"
  for app in "${APPS[@]}"; do
    cargo build --release -p "$app"
  done
fi

TARGET_DIR="$(dist_target_dir)"

for app in "${APPS[@]}"; do
  exe="$TARGET_DIR/release/$app$EXT"
  if [ ! -f "$exe" ]; then
    echo "dist: $exe was not produced — build first, or drop --no-build" >&2
    exit 1
  fi

  version="$(dist_version "$exe")"
  folder="$(dist_dir "$app" "$PLATFORM")/$app-$version-$TRIPLE"
  dist_fresh_dir "$folder"

  cp "$exe" "$folder/"
  cp LICENSE-MIT LICENSE-APACHE "$folder/"
  readme "$app" "$version" > "$folder/README.txt"

  echo "dist: staged $folder"

  if [ "$ZIP" = 1 ]; then
    archive="$(dist_dir "$app" "$PLATFORM")/$app-$version-$TRIPLE.zip"
    rm -f "$archive"
    ( cd "$(dirname "$folder")" && zip -qr "$(basename "$archive")" "$(basename "$folder")" )
    echo "dist: wrote $archive"
  fi
done
