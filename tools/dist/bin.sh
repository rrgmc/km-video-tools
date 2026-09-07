#!/usr/bin/env bash
#
# Stages one folder holding every executable this platform can build.
#
#   tools/dist/bin.sh                  # stage everything, then gather it into dist/bin{,-console}/
#   tools/dist/bin.sh --no-build       # gather what is already staged; build nothing
#   tools/dist/bin.sh --zip            # also produce a versioned .zip of each folder
#   tools/dist/bin.sh -v               # watch the builds; quiet is the default
#
#   dist/bin/<platform>/           the GUI form of anything that has one, the plain form of the rest
#   dist/bin-console/<platform>/   the console form of anything that has one, the same plain rest
#
# **This is a second carrier, not a new layout.** `dist/<app>/<platform>/<app>-<version>-<triple>/`
# is untouched and stays the answer to what a release *is*: a thing you hand to somebody, and
# somebody who wants only the command line has no use for the window. What this answers is the other
# question people ask -- *give me one folder with all of it in it* -- which today means unpacking two
# folders and merging them by hand, and the merge has a trap in it: both folders carry a `README.txt`
# and a pair of licence files under the same names, so a plain copy silently keeps one README and
# loses the other.
#
# **Executables only.** No installer and no archive of another carrier -- those are carriers of their
# own, and a folder of programs is not the place to keep one.
#
# **It gathers rather than builds, and that is the property to preserve.** Every fact about what a
# staged folder holds, what each README says and how the macOS bundle is made already lives in
# tools/dist/cmd.sh. None of it is restated here: this script runs that one and copies what it
# produced, so the two cannot drift. What it knows by itself is three rules about *shape*:
#
#   1. A `*.app` beside the folder is that product's GUI form                  -> bin/ only
#      ...and the bare executable in the folder is therefore the console form  -> bin-console/ only
#   2. A file `<x>-console<EXT>` is the console form                           -> bin-console/ only
#      ...and its sibling `<x><EXT>` is therefore the GUI form                 -> bin/ only
#   3. Every other executable is single-form                                   -> both
#
# **Rules 1 and 2 are the same rule, said from each end.** A product with two forms puts the one you
# double-click in `bin/` and the one with somewhere to print in `bin-console/`; what differs is only
# how the platform spells the pair. Windows spells it as two files under two names,
# `km-video-downloader.exe` beside `km-video-downloader-console.exe`. macOS spells it as a bundle
# beside the executable it wraps, so `KM Video Downloader.app` is the GUI form and the bare
# `km-video-downloader` in the staged folder is the console one -- and copying that bare binary into
# `bin/` as well would put a second, worse way to start the same program next to the icon, in the
# folder whose own README says it holds the one you double-click.
#
# Support files -- the two licence texts -- go to both. `README.txt` is renamed `README-<app>.txt`,
# because the two collide on one name and each is a document somebody actually reads; a short
# `README.txt` written here says what the folder is and points at them.
#
# **Rules and not a table, so that a third program needs no edit here.** The alternative was a list
# naming each product's GUI and console executable, which is the same list tools/dist/cmd.sh already
# keeps in a form it can act on, spelled a second time in a form it cannot.
#
# **Nothing is dropped silently.** Anything a staged folder holds that matches none of the rules is
# reported by name and stops the run. A carrier that quietly loses a file is the one failure this
# must not have.
#
# **The folders carry no version, and the zips do.** `dist/bin/windows` is a place you keep the
# current build, in the way `KM Video Downloader.app` is; the number belongs on the thing you hand
# over, which is the archive.

set -euo pipefail

cd "$(dirname "$0")/../.."

. tools/dist/common.sh

BUILD=1
ZIP=0
VERBOSE=0
APPS=(km-video-fetch km-video-downloader)

for arg in "$@"; do
  case "$arg" in
    --no-build) BUILD=0 ;;
    --zip) ZIP=1 ;;
    -v|--verbose) VERBOSE=1 ;;
    -h|--help)
      echo "usage: tools/dist/bin.sh [--no-build] [--zip] [-v]"
      exit 0 ;;
    *) echo "dist-bin: unknown option $arg" >&2; exit 2 ;;
  esac
done

if [ "$BUILD" -eq 1 ]; then
  echo "dist-bin: staging every program"
  if [ "$VERBOSE" -eq 1 ]; then
    tools/dist/cmd.sh
  else
    log="$(mktemp)"
    if ! tools/dist/cmd.sh > "$log" 2>&1; then
      cat "$log" >&2; rm -f "$log"
      echo "dist-bin: staging failed; the log is above." >&2
      exit 1
    fi
    rm -f "$log"
  fi
fi

PLATFORM="$(dist_platform)"
TRIPLE="$(dist_host_triple)"
EXT="$(dist_exe_ext)"
BIN="dist/bin/$PLATFORM"
CONSOLE="dist/bin-console/$PLATFORM"

staged_dir() { # <app>
  local app="$1" match
  for match in "$(dist_dir "$app" "$PLATFORM")/$app-"*"-$TRIPLE"; do
    [ -d "$match" ] && { printf '%s' "$match"; return 0; }
  done
  return 1
}

for app in "${APPS[@]}"; do
  if ! staged_dir "$app" >/dev/null; then
    echo "dist-bin: nothing staged for $app under $(dist_dir "$app" "$PLATFORM")." >&2
    if [ "$BUILD" -eq 0 ]; then
      echo "          --no-build was given, so nothing was staged for it here either. Run this" >&2
      echo "          script without it, or tools/dist/cmd.sh first." >&2
    fi
    exit 1
  fi
done

dist_fresh_dir "$BIN"
dist_fresh_dir "$CONSOLE"

# **Is this thing a program?** On Windows the answer is the extension and nothing else: every file in
# a staged folder reports as executable to a Git Bash `-x`, so the mode says nothing at all there.
is_executable() { # <path>
  if [ -n "$EXT" ]; then
    case "$1" in *"$EXT") return 0 ;; *) return 1 ;; esac
  fi
  [ -f "$1" ] && [ -x "$1" ]
}

VERSION=""
unaccounted=()
gui_count=0
console_count=0

for app in "${APPS[@]}"; do
  src="$(staged_dir "$app")"

  # Rule 1's first half, asked before the loop because it decides what the bare executable *is*: a
  # bundle beside the folder's own binary means this product has two forms on this platform.
  has_bundle=0
  for candidate in "$src"/*.app; do
    [ -d "$candidate" ] && has_bundle=1
  done

  for entry in "$src"/*; do
    base="$(basename "$entry")"

    case "$base" in
      *.app)
        # Rule 1: the bundle is the GUI form, and only the GUI folder gets it.
        cp -R "$entry" "$BIN/$base"
        gui_count=$((gui_count + 1))
        continue ;;
      README.txt)
        # Renamed rather than merged: two of these collide on one name, and each is read.
        cp "$entry" "$BIN/README-$app.txt"
        cp "$entry" "$CONSOLE/README-$app.txt"
        continue ;;
      LICENSE-*)
        # Identical in both staged folders; the second copy overwrites the first and says the same.
        cp "$entry" "$BIN/$base"
        cp "$entry" "$CONSOLE/$base"
        continue ;;
    esac

    if ! is_executable "$entry"; then
      unaccounted+=("$app/$base")
      continue
    fi

    case "$base" in
      *-console"$EXT")
        # Rule 2: the console half of a Windows pair.
        cp "$entry" "$CONSOLE/$base"
        console_count=$((console_count + 1)) ;;
      *)
        if [ "$has_bundle" -eq 1 ]; then
          # Rule 1's second half: a bundle exists, so this bare binary is the console form and the
          # GUI folder must not also get it -- see the header.
          cp "$entry" "$CONSOLE/$base"
          console_count=$((console_count + 1))
        elif [ -e "${entry%"$EXT"}-console$EXT" ]; then
          # Rule 2 from the other end: a `-console` sibling exists, so this is the GUI form.
          cp "$entry" "$BIN/$base"
          gui_count=$((gui_count + 1))
        else
          # Rule 3: one form, and both folders get it.
          cp "$entry" "$BIN/$base"
          cp "$entry" "$CONSOLE/$base"
        fi ;;
    esac

    # The version, from the first single-form executable that can answer for it -- from a binary and
    # never a manifest, the same rule every other script here follows.
    if [ -z "$VERSION" ] && [ "$app" = km-video-fetch ]; then
      VERSION="$(dist_version "$entry")"
    fi
  done
done

if [ "${#unaccounted[@]}" -gt 0 ]; then
  echo "dist-bin: a staged folder holds files none of the three rules covers:" >&2
  printf '            %s\n' "${unaccounted[@]}" >&2
  echo "          Give them a rule or subtract them by name. Nothing is dropped from a carrier" >&2
  echo "          by accident." >&2
  exit 1
fi

[ -n "$VERSION" ] || { echo "dist-bin: could not read a version out of any staged executable." >&2; exit 1; }

# -- the document each folder gets -----------------------------------------------------------------
#
# Short on purpose. The per-program READMEs beside it are the ones that say how each program is used;
# this one exists to say what *this folder* is, which is the one thing they cannot, and to point at
# them by name.
write_readme() { # <dir> <gui|console>
  local dir="$1" kind="$2"
  {
    cat <<'HEAD'
KM Video Tools
==============

One folder holding every program this build produced. The per-program documents beside this one --
README-km-video-fetch.txt and README-km-video-downloader.txt -- are what say how each is used; this
says only what the folder is.

HEAD

    if [ "$kind" = gui ]; then
      cat <<'BODY'
This is the folder of programs you start by double-clicking, where a program has that form.
BODY
      if [ "$PLATFORM" = windows ]; then
        cat <<'BODY'

km-video-downloader.exe opens its own window and no console beside it. If you want one that prints
to a shell -- for --help, or to watch what it is doing -- that is the copy in the bin-console folder.
BODY
      elif [ "$PLATFORM" = macos ]; then
        cat <<'BODY'

"KM Video Downloader.app" is the one with an icon, a Dock entry and a menu bar. The bare
km-video-downloader in the bin-console folder is the same program with somewhere to print.

The bundle is not signed with a Developer ID, so on any Mac other than the one that built it macOS
refuses to open it until the quarantine flag is cleared:

    xattr -dr com.apple.quarantine "KM Video Downloader.app"
BODY
      fi
    else
      cat <<'BODY'
This is the folder of programs that have somewhere to print: run them from a shell and they will
report there. Where a program has only one form, it is the same file as in the bin folder.
BODY
    fi

    cat <<'TAIL'

Neither folder carries a version in its name, because it is a place you keep the current build. The
number is on the archive, which is the thing you hand to somebody.

Neither program does the downloading: that is yt-dlp, from https://github.com/yt-dlp/yt-dlp, which
has to be on your PATH. Add ffmpeg too if you want the check on whether what arrived is playable.
TAIL
  } > "$dir/README.txt"
}

write_readme "$BIN" gui
write_readme "$CONSOLE" console

echo "dist: gathered $BIN"
echo "dist: gathered $CONSOLE"
echo "      version   $VERSION"
echo "      gui       $gui_count with a windowed form"
echo "      console   $console_count with a printing form"

# -- the archives ----------------------------------------------------------------------------------
#
# Renamed on the way in, because a zip unpacking to `windows/` tells its recipient nothing. A folder
# is a place you keep the current build; an archive is a thing you hand over, and it carries the
# version for the same reason the staged folders do.
if [ "$ZIP" -eq 1 ]; then
  TARGET="${TRIPLE%%-*}"
  dist_zip_as "$BIN"     "dist/bin/km-video-tools-bin-$VERSION-$TARGET"
  dist_zip_as "$CONSOLE" "dist/bin-console/km-video-tools-bin-console-$VERSION-$TARGET"
fi
