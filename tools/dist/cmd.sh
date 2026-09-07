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
# should not be handed over by accident.
#
# **One script for both programs, because both are one executable and a README.** `km-video-downloader`
# serves a page rather than printing lines, and that changes nothing about what is handed over: its
# whole user interface is compiled into the same single file. A script of its own would be worth
# writing the day one of these grows a `.app` bundle or a folder of libraries beside it.
#
# **The README is written here rather than committed.** It is the document a person reading the
# folder actually opens, it has to name the version and the platform they were given, and there is no
# second copy of it to drift.

set -euo pipefail

cd "$(dirname "$0")/../.."

. tools/dist/common.sh


# The macOS application bundle.
#
# **A directory with a plist in it, and that is the whole of what an application is on that
# platform.** Without one there is no Dock icon, no name in the menu bar and nothing to
# double-click — the binary runs and opens a window, but as an anonymous process.
#
# `LSUIElement` is deliberately *not* set: this is an application somebody looks at, not an agent.
# `NSHighResolutionCapable` is what stops the webview being drawn at 1x and scaled up.
#
# Unsigned, and that is worth saying out loud: macOS will refuse a downloaded copy until it is
# opened once from the context menu, or `xattr -d com.apple.quarantine` is run over it. A copy built
# on the machine it runs on has no quarantine attribute and is not affected.
bundle() { # <app> <version> <folder>
  local app="$1" version="$2" folder="$3"
  local name; name="$(display_name "$app")"
  local root="$folder/$name.app"

  rm -rf "$root"
  mkdir -p "$root/Contents/MacOS" "$root/Contents/Resources"

  cp "$folder/$app" "$root/Contents/MacOS/$app"
  cp "icon/$app.icns" "$root/Contents/Resources/$app.icns"

  cat > "$root/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleDisplayName</key><string>$name</string>
  <key>CFBundleIdentifier</key><string>com.rrgmc.$app</string>
  <key>CFBundleExecutable</key><string>$app</string>
  <key>CFBundleIconFile</key><string>$app</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

  echo "dist: bundled $root"
}

# What a program is called where a person reads it rather than types it.
#
# The bundle's name, and the one macOS draws in the menu bar. Everything else — the executable, the
# folder, the command somebody types — keeps the hyphenated form.
display_name() { # <app>
  case "$1" in
    km-video-downloader) printf 'KM Video Downloader' ;;
    km-video-fetch)      printf 'km-video-fetch' ;;
    *)                   printf '%s' "$1" ;;
  esac
}

ALL_APPS=(km-video-fetch km-video-downloader)

# The document in the folder. Dispatched by name, so a program with no README here is a hard failure
# rather than a folder that quietly ships without one.
readme() { # <app> <version>
  case "$1" in
    km-video-fetch)      readme_km_video_fetch "$2" ;;
    km-video-downloader) readme_km_video_downloader "$2" ;;
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
into it, as km-video-fetch.kmvf, which is read when no URLs and no --from-file are given.

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

readme_km_video_downloader() { # <version>
  cat <<README
km-video-downloader $1
$(printf '=%.0s' $(seq 1 $((20 + ${#1}))))

The same fetching as km-video-fetch, with a window instead of a command line. Set a folder once,
paste the links or pick a file of them, press Fetch, and watch it happen.

There are two executables here. Double-click **km-video-downloader**; it opens its own window and no
console. **km-video-downloader-console** is the same program from a shell, where --help and the
address it is serving at have somewhere to be read.

What you need first
-------------------

yt-dlp, and ffmpeg. Neither is included here and neither is downloaded for you.

    yt-dlp        https://github.com/yt-dlp/yt-dlp -- pipx, winget, brew or your package manager
    ffmpeg        yt-dlp needs it to join the video and audio streams and to write the tags.
                  ffprobe, from the same package, is what reads the file back afterwards

Running it
----------

    km-video-downloader

opens the window. --browser uses a browser tab instead, and --open opens one alongside. The page is
at http://127.0.0.1:8181/ either way; closing the window stops the program, as does Ctrl-C in the
console build.

It listens on this computer only. There is no password on it, and it writes files as you --- so
--lan, which makes it reachable from the rest of your network, is for a network you trust and only
while you need it.

Options
-------

    --port N                  the port to listen on, default 8181
    --lan                     listen on every interface, not only this computer
    --open                    open a browser once it is listening
    --browser                 use a browser tab rather than this program's own window
    --data-dir PATH           where the remembered folder and options are kept
    --yt-dlp PATH             the yt-dlp to run, when it is not on the PATH
    -v, --verbose             say more; twice for a great deal more
    --version

What it remembers
-----------------

The output folder, and the options beside it --- playlist, re-encode, subtitles, the playlist limit
and the browser to take cookies from. They come back next time it starts. "Fetch again" and "say
what would be fetched" are not remembered, because both are things you do once.

The file is settings.json in this platform's own config directory for km-video-downloader, or in
--data-dir where one was given.

What it fetches, and from where, is your business
-------------------------------------------------

This runs yt-dlp against whatever you point it at. Downloading from a site may be contrary to that
site's terms, and the videos are somebody else's copyrighted work. That is a matter for whoever runs
the download.

Licenses
--------

km-video-downloader is MIT OR Apache-2.0, at your option; both texts are beside this file. It carries
a copy of htmx (0BSD), served at /static/htmx-LICENSE.txt while it is running. yt-dlp and ffmpeg are
separate programs under their own licenses and are not included here.
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

  # **The console twin, where the program has one.** `km-video-downloader` is GUI-subsystem on
  # Windows so a double-click gives an application rather than a black window -- which also means it
  # has nowhere to print, so `--help` and anything that goes wrong need the second executable.
  # Staged by looking for it rather than from a list: a program either built one or did not.
  console="$TARGET_DIR/release/$app-console$EXT"
  if [ -f "$console" ]; then
    cp "$console" "$folder/"
  fi

  # ...and on macOS the same executable again inside a bundle, which is the only form the platform
  # gives a Dock icon, a name in the menu bar, and a double-click.
  if [ "$PLATFORM" = macos ] && [ -f "icon/$app.icns" ]; then
    bundle "$app" "$version" "$folder"
  fi

  echo "dist: staged $folder"

  if [ "$ZIP" = 1 ]; then
    dist_zip "$(dirname "$folder")" "$(basename "$folder")"
  fi
done
