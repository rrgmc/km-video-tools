#!/usr/bin/env bash
#
# The few facts every staging script here needs, in one place.
#
# Sourced, never run. `. tools/dist/common.sh` from a script that has already `cd`ed to the
# repository root.

# **Never spell `target/release/<x>`.** The build directory moves — a `CARGO_TARGET_DIR` in the
# environment, a `build.target-dir` in a config file, or a shared directory somebody set up for two
# checkouts — and a hard-coded path produces the worst failure this kind of script has: cargo prints
# `Finished` and the next line says the executable was not produced.
#
# The result is memoised by the *caller* storing it, not here. A command substitution is a subshell,
# so a cache written inside one is gone before the assignment completes.
dist_target_dir() { # -> prints the directory cargo builds into
  local dir
  dir="$(cargo metadata --format-version 1 --no-deps \
         | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
  if [ -z "$dir" ]; then
    echo "dist: could not ask cargo where it builds" >&2
    return 1
  fi
  # cargo reports a Windows path with escaped backslashes. Git Bash wants forward ones.
  case "$dir" in
    [A-Za-z]:*) dir="$(printf '%s' "$dir" | sed 's|\\\\|/|g')" ;;
    *)          dir="$(printf '%s' "$dir" | sed 's|\\\\|\\|g')" ;;
  esac
  printf '%s' "$dir"
}

dist_host_triple() { # -> prints this host's target triple
  rustc -vV | sed -n 's/^host: //p'
}

dist_platform() { # [triple, default: this host]
  case "${1:-$(dist_host_triple)}" in
    *windows*) printf 'windows' ;;
    *darwin*)  printf 'macos' ;;
    *linux*)   printf 'linux' ;;
    *)         printf 'unknown' ;;
  esac
}

dist_exe_ext() { # [triple, default: this host]
  case "${1:-$(dist_host_triple)}" in *windows*) printf '.exe' ;; *) printf '' ;; esac
}

# The oldest macOS a build of this can be handed to, which is what the wry/tao stack needs.
#
# **Two places state it and one of them checks.** This is the number `tools/dist/cmd.sh` writes into
# a bundle's `LSMinimumSystemVersion`; `tools/platform/macos/pkg/distribution.xml` states the same
# floor for the installer, in an XML attribute no script can derive it from, and
# `tools/platform/macos/installer.sh` fails on a disagreement. Two statements and an assertion is
# the arrangement `rust-toolchain.toml` and `Cargo.toml` already have here, for the same reason: a
# number in two files is a drift waiting to happen unless something reads both.
dist_min_macos() { # -> prints the minimum macOS version
  printf '11.0'
}

# The one place the layout is written down:
#
#   dist/<app>/<platform>/<app>-<version>-<triple>/
#
# The folder carries the version and the triple because it is a thing you hand to somebody, and what
# they will ask first is which build they were given. `dist/` itself is git-ignored — it is output.
dist_dir() { # <app> <platform>
  printf 'dist/%s/%s' "$1" "$2"
}

# The workspace's version, out of the manifest and without building anything.
#
# **The one place a version is read from the manifest rather than from a binary**, and it is not the
# exception to `dist_version()` below that it looks like: that answers "what is this artifact", this
# answers "which artifact did we mean". Asking for one member is asking for all of them, because every
# crate here is `version.workspace = true`.
#
# Two strips, because `cargo pkgid` has two output shapes: `...#km-video-fetch@1.8.0` when the package
# name differs from its directory, and `...#1.8.0` when it does not. `##*@` is a no-op on the second,
# which is the shape this workspace actually produces.
dist_pkg_version() { # -> prints the workspace version
  local p
  p="$(cargo pkgid -p km-video-fetch)"
  p="${p##*#}"
  printf '%s' "${p##*@}"
}

# The staged folder for one app on one platform: `dist_dir()`'s rule with the current version in it.
#
# **It names the folder rather than searching for one, and that is the whole point.** This was two
# private copies -- one in tools/dist/bin.sh, one in tools/platform/macos/installer.sh -- which globbed
# `<app>-*-<triple>` and returned the *first* match. A glob expands in sorted order, so with 1.7.0 and
# 1.8.0 both staged it returned 1.7.0, and everything downstream stayed consistent about it: bin.sh
# gathered the old executables, read the version out of one of *them*, and both setup programs then
# read it out of that payload in turn. The result was an installer correctly labelled `1.7.0` for a
# build nobody asked for, on both platforms, with no failure anywhere to notice. `dist_fresh_dir`
# clears only the folder it is about to write, so the loser of that comparison survives for ever --
# `task clean:old` is what takes it away, and this is what stops it mattering.
#
# **Selecting by the manifest does not break the "from a binary, never a manifest" rule** that
# tools/dist/bin.sh, both installers and `dist_version()` below all state. The manifest chooses *which
# folder*; the binary inside it still says *what it is*, and the two cannot disagree, because
# tools/dist/cmd.sh built that folder's name out of that binary's own `--version` in the first place.
# What changes is only the failure: a current build that is not staged now says so, where before it
# was silently replaced by an older one.
dist_staged_dir() { # <app> <platform>  -> prints the staged folder, or fails
  local dir
  dir="$(dist_dir "$1" "$2")/$1-$(dist_pkg_version)-$(dist_host_triple)"
  [ -d "$dir" ] || return 1
  printf '%s' "$dir"
}

# The version out of the built executable rather than out of `Cargo.toml`.
#
# Asked of the artifact so the number in the folder name is the number the program will print — a
# staging run against a stale build says so instead of mislabelling it.
dist_version() { # <path to executable>
  local exe="$1" version cmd
  case "$exe" in
    /* | [A-Za-z]:[/\\]*) cmd="$exe" ;;
    *)                    cmd="./$exe" ;;
  esac
  version="$("$cmd" --version | awk '{print $2}')"
  if [ -z "$version" ]; then
    echo "dist: could not read a version out of $exe --version" >&2
    return 1
  fi
  printf '%s' "$version"
}

# Cleared rather than merged, so a file that stopped being shipped cannot survive a rerun and quietly
# go out in the next archive.
dist_fresh_dir() { # <path>
  rm -rf "$1"
  mkdir -p "$1"
}

# `zip` first, PowerShell second: `zip` is there on macOS and Linux, and PowerShell is what a stock
# Windows box has instead -- Git Bash ships no `zip`, so on Windows the first branch is never the one
# taken. Neither being present leaves the folder in place and says so, because the folder is the
# deliverable and the archive is a convenience.
#
# The same fallback karaokemachine's `dist_zip()` makes, for the same reason.
dist_zip() { # <parent dir> <folder name>
  local parent="$1" name="$2"
  rm -f "$parent/$name.zip"
  if command -v zip >/dev/null 2>&1; then
    ( cd "$parent" && zip -qr "$name.zip" "$name" )
  elif command -v powershell >/dev/null 2>&1; then
    # `host_path` and not the bare path: PowerShell is a Windows program, and a Git Bash absolute
    # path reaches it as `\tmp\tmp.XXXX`, which it reports as a path that does not exist. A relative
    # path happens to survive, which is why this only surfaced once an archive was built out of a
    # temp directory rather than out of dist/.
    local hp; hp="$(host_path "$parent")"
    powershell -NoProfile -Command \
      "Compress-Archive -Force -Path '$hp/$name' -DestinationPath '$hp/$name.zip'"
  else
    echo "dist: neither zip nor powershell is on PATH, so no archive was made. The folder above is"
    echo "      complete; compress it by hand."
    return 0
  fi
  echo "dist: wrote $parent/$name.zip"
}

# A path in the form a Windows program will understand, for the one case Git Bash cannot help with:
# an argument that is a path but does not look like one to the shell.
#
# `MSYS2_ARG_CONV_EXCL` stops the mangling for the argument as a whole; this is what makes what is
# left of it correct, turning `/c/prog/...` into `C:/prog/...`. Both are needed to hand a path to
# ISCC.exe through a `/D` define, and neither is needed anywhere else.
host_path() { # <path>
  if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi
}

# -- the document an installed build gets ----------------------------------------------------------
#
# **The one README this file writes, and the exception to the rule that it writes none.** The two in
# tools/dist/cmd.sh describe a folder somebody unpacked, they differ in every word, and a shared one
# with six substitutions would be worse than two honest ones.
#
# This is not one of those. It is what an *installed* build gets, it has exactly two callers -- the
# Windows setup program and the macOS one -- and what those two have to agree about is facts rather
# than prose. Two installers describing how to remove the same product differently is what one copy
# prevents, and it is invisible when it happens, because nobody opens an installed README until
# something has already gone wrong.
dist_installed_readme() { # <windows|macos>
  cat <<'HEAD'
KM Video Tools
==============

Two programs for getting karaoke video songs onto disk, in the shape a karaoke package wants them:

  KM Video Downloader   The window. Set a folder once, paste the links or pick a file of
                        them, press Fetch, and watch it happen.

  km-video-fetch        The same fetch on the command line, for a script or a shell.

They are the same program twice and differ only in whether the progress becomes lines or a bar.

What you need beside them
-------------------------

**yt-dlp**, and it is not installed by this setup. It is the thing that does the downloading; these
programs are the selector, the tags and the check around it. Get it from
https://github.com/yt-dlp/yt-dlp and put it on your PATH.

A copy more than a few months old fails in ways that look like a broken network, because the sites
move and yt-dlp follows them. Both programs print the version they found and say so when it is stale.

**ffprobe**, if you want the check on what arrived. It comes with ffmpeg, from https://ffmpeg.org.
Without it the download still happens; only the "is this actually playable" answer is missing.

HEAD

  case "$1" in
    windows)
      cat <<'WINDOWS'
Where it is
-----------

This was installed for your account only, under

  %LOCALAPPDATA%\Programs\KM Video Tools

so it needed no administrator password and it is not visible to other accounts on this computer.

If you ticked the PATH box, `km-video-fetch` can be typed in any new console window. A console that
was already open when you installed will not have it -- open a new one.

If you ticked the .kmvf box, double-clicking a list of links opens KM Video Downloader with that
list filled in and the output folder set to the folder the list is in. Nothing is fetched until you
press Fetch, and a second one opened while the window is up goes to that window rather than starting
a second copy.

Removing it
-----------

Settings, then Apps, then Installed apps: find "KM Video Tools" and choose Uninstall. There is also
an entry in the Start menu folder. The uninstaller takes the PATH entry and the .kmvf association
back out.

**Your videos are not touched.** Neither is the download folder you chose, wherever you put it.
Settings live in %APPDATA%\km-video-downloader and are left alone too; delete that folder by hand if
you want them gone.

A note on the warning you may have seen
---------------------------------------

This setup program is not signed with a purchased certificate, so Windows SmartScreen shows
"Windows protected your PC" the first time it runs. More info, then Run anyway, is the way past it.
WINDOWS
      ;;
    macos)
      cat <<'MACOS'
Where it is
-----------

KM Video Downloader is in /Applications, and km-video-fetch is in /usr/local/km-video-tools with a
symlink in /usr/local/bin -- which is already on your PATH, so there is nothing to add to a .zshrc
and nothing was added to one.

Opening a list
--------------

KM Video Downloader says it opens .kmvf files, so double-clicking a list of links opens it with that
list filled in and the output folder set to the folder the list is in. Nothing is fetched until you
press Fetch, and a second one opened while the window is up goes to that window.

There was no box to tick for this: the application declares the file type and macOS notices when it
is in /Applications. Removing the application removes the association with it.

Removing it
-----------

Drag KM Video Downloader from /Applications to the Trash, and delete /usr/local/km-video-tools and
the /usr/local/bin/km-video-fetch symlink. The second needs an administrator password, the same one
the install asked for.

**Your videos are not touched.** Neither is the download folder you chose. Settings live in
~/Library/Application Support/km-video-downloader and are left alone too.

A note on the warning you may have seen
---------------------------------------

This package is not signed with a Developer ID, so Gatekeeper refuses it on a first double-click.
Control-click the .pkg and choose Open, or allow it under System Settings, Privacy & Security.
MACOS
      ;;
    *)
      echo "dist: no installed README is written for $1" >&2
      return 1
      ;;
  esac
}

# The same zip, for a folder whose own name is not the name the archive should carry.
#
# **A zip's top-level entry is a directory name somebody is going to be looking at**, and that is the
# whole reason this exists rather than a `mv` after the fact. `tools/dist/bin.sh` gathers into
# `dist/bin/<platform>`, because that is what the folder is *called* -- but an archive of it unpacking
# to `windows/` tells its recipient nothing, so it is renamed on the way in.
dist_zip_as() { # <folder> <archive path without .zip>
  local folder="$1" out="$2" parent name tmp
  parent="$(dirname "$out")"
  name="$(basename "$out")"
  mkdir -p "$parent"
  tmp="$(mktemp -d)"
  cp -R "$folder" "$tmp/$name"
  # Quiet, because `dist_zip` names the file it wrote and the file it writes here is a temporary one
  # in a directory nobody will look in. The line worth printing is the one below.
  dist_zip "$tmp" "$name" >/dev/null
  rm -f "$out.zip"
  mv "$tmp/$name.zip" "$out.zip"
  rm -rf "$tmp"
  echo "dist: wrote $out.zip"
}
