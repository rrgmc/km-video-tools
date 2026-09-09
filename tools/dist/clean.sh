#!/usr/bin/env bash
#
# Takes staged releases away again.
#
#   tools/dist/clean.sh --all              # every staged release under dist/
#   tools/dist/clean.sh --old              # only the ones that are not this workspace's version
#   tools/dist/clean.sh --old --dry-run    # ...say what that would remove, remove nothing
#
# The counterpart of the staging scripts, and it exists as a script rather than as a line in
# Taskfile.yml for a reason that is not style: Task runs its commands through an embedded POSIX
# shell, which gives it `for`, `case` and parameter expansion on every platform but **no `rm`** --
# that is an external command, and there is no `rm.exe` on the Windows box this is mostly developed
# on, any more than there is a `sed` or a `grep`. `clean: rm -rf dist` was in the Taskfile until this
# script existed, and from a PowerShell or a cmd it was
#
#     "rm": executable file not found in $PATH
#     task: Failed to run task "clean": exit status 127
#
# every time. It appeared to work only from a Git Bash, which is the one shell whose PATH lends Task
# a coreutils it does not otherwise have -- so the task was broken in exactly the situation the
# Taskfile's header says the SH variable exists for. So the Taskfile chooses which script to run,
# which is what it already does for staging, and the deleting happens here where coreutils exist.
#
# It also means `task` stays optional. Nothing in this repository requires it, and this is the
# command the Taskfile's `clean`, `clean:old` and half of `clean:all` are.
#
# **The version is read from the manifest here, not from the artifact.** That is the opposite of
# `dist_version()` in tools/dist/common.sh, deliberately: its method is to run the executable and
# read its `--version`, and half of what this walks is a `.zip` or an installer with nothing to run.
# The two cannot disagree, because a clap `version` is CARGO_PKG_VERSION and that is what
# `cargo pkgid` prints.

set -euo pipefail

cd "$(dirname "$0")/../.."
[ -f Cargo.toml ] && [ -d crates ] || { echo "${0##*/}: not at the repository root -- landed in $PWD" >&2; exit 1; }

# **`tools/dist/common.sh` is deliberately not sourced here**, though every staging script does. Its
# helpers all *build* the layout -- name a folder, clear it, find the staged one, stage assets into
# it -- and this script is the only one that takes the layout apart, which it does by walking `dist/`
# and reading the names rather than by constructing any. Sourcing it to use nothing from it would
# suggest a coupling that is not there. The coupling that *is* there is the rule itself: `dist_dir()`
# in that file owns `dist/<app>/<platform>/<app>-<version>-<triple>`, and if it changes, this changes.

MODE=""
DRY=0
for arg in "$@"; do
  case "$arg" in
    --all) MODE=all ;;
    --old) MODE=old ;;
    --dry-run|-n) DRY=1 ;;
    -h|--help)
      echo "usage: tools/dist/clean.sh (--all | --old) [--dry-run]"
      exit 0
      ;;
    *) echo "dist-clean: unknown option $arg" >&2; exit 2 ;;
  esac
done

if [ -z "$MODE" ]; then
  echo "dist-clean: say which -- --all (everything) or --old (everything but the current version)" >&2
  exit 2
fi

if [ ! -d dist ]; then
  echo "dist-clean: nothing to clean -- dist/ does not exist"
  exit 0
fi

REMOVED=0

take() { # <path>
  if [ "$DRY" -eq 1 ]; then
    echo "  would remove $1"
  else
    echo "  removing $1"
    rm -rf "$1"
  fi
  REMOVED=$((REMOVED + 1))
}

# -- everything ------------------------------------------------------------------------------------

if [ "$MODE" = all ]; then
  for d in dist/*/; do
    [ -d "$d" ] || continue
    take "${d%/}"
  done
  # Only if it came out empty. A stray file somebody put there is theirs, not ours to guess about --
  # which is the one way this differs from the `rm -rf dist` it replaces, and it differs on purpose.
  [ "$DRY" -eq 1 ] || rmdir dist 2>/dev/null || true
  echo "dist-clean: $REMOVED staged folder(s)"
  exit 0
fi

# -- everything but the current version --------------------------------------------------------------

# Two strips, because `cargo pkgid` has two output shapes: `...#km-video-fetch@1.8.0` when the package
# name differs from its directory, and `...#1.8.0` when it does not. `##*@` is a no-op on the second,
# which is the shape this workspace actually produces.
pkg_version() { # <cargo pkgid arguments>  -> prints the version
  local p
  p="$(cargo pkgid "$@")"
  p="${p##*#}"
  printf '%s' "${p##*@}"
}

# **One number, for everything under `dist/`.** Every crate here is `version.workspace = true`, so no
# product has a version of its own -- and holding one that did to the workspace's number would delete
# the current build of it every time this ran, while reporting it had removed something stale, which
# is the one mistake a cleaner must not make. Asking for one member is asking for all of them.
VERSION="$(pkg_version -p km-video-fetch)"

# An entry is removed only when its name **begins with its app's own name followed by a version that
# is not the wanted one**. Anything unrecognized is left alone rather than guessed at, which is what
# makes four things safe without special-casing any of them:
#
#   - `dist/bin/<platform>` and `dist/bin-console/<platform>`, which tools/dist/bin.sh stages under a
#     versionless name on purpose -- a folder is where you keep the current build, and the number
#     goes on the archive you hand over. So `--old` can never take one of those; their *zips* it can,
#     and does, at the bottom of this file.
#   - `dist/setup/windows/generated`, versionless for the same kind of reason, and which
#     tools/platform/windows/installer.sh clears itself on every run.
#   - `KM Video Downloader.app`, which carries no version at all -- the number is in its Info.plist,
#     by tools/dist/cmd.sh's own decision. It does not begin with an app's name either, so it is
#     never matched. It lives *inside* a versioned folder in any case, so it goes when that goes.
#   - the contents of a folder that is being kept -- `km-video-downloader-console.exe`,
#     `LICENSE-MIT`, `README.txt` -- none of which parse as a version.
#
# **One thing this therefore cannot see**: a whole product folder for a program this repository has
# stopped building. Nothing stages into it, so no staging run can clean it, and the version inside it
# is current, so `--old` reads it as this build. Knowing it is dead means knowing which products exist,
# which is a list this script deliberately does not keep -- see the note above about not sourcing
# tools/dist/common.sh. So it is `--all`, or `rm -rf` by hand, and saying so here is the whole of the
# fix.
#
# One separator rather than the karaoke app's two: everything here is `<app>-<version>-<triple>`,
# and the `_` its `sweep` also strips is for a Debian package this repository does not build.
sweep() { # <app> <wanted version> <path>...
  local app="$1" want="$2" e base rest v
  shift 2
  for e in "$@"; do
    [ -e "$e" ] || continue
    base="$(basename "$e")"
    rest="${base#"$app"}"
    [ "$rest" != "$base" ] || continue
    rest="${rest#-}"
    v="${rest%%-*}"
    case "$v" in [0-9]*.[0-9]*) ;; *) continue ;; esac
    [ "$v" != "$want" ] || continue
    take "$e"
  done
}

for appdir in dist/*/; do
  [ -d "$appdir" ] || continue
  app="$(basename "$appdir")"
  for platdir in "$appdir"*/; do
    [ -d "$platdir" ] || continue
    sweep "$app" "$VERSION" "$platdir"*
  done
done

# tools/dist/bin.sh's archives, which sit one level higher than everything the loop above walks:
# `dist/bin/km-video-tools-bin-<version>-<arch>.zip`, beside the versionless `<platform>/` folder
# rather than inside it. They are swept here for exactly that reason -- the loop descends to
# `dist/<app>/<platform>/*`, and these are at `dist/<app>/*`.
#
# The folders themselves are left alone, deliberately; see the first bullet above `sweep`.
sweep km-video-tools-bin         "$VERSION" dist/bin/*
sweep km-video-tools-bin-console "$VERSION" dist/bin-console/*

# The Windows setup program, `dist/setup/windows/km-video-tools-setup-<version>-x86_64.exe`. It sits
# where the loop above already walks -- but the loop passes the *directory* name as the app, so it
# looks for something beginning `setup-` and this file begins `km-video-tools-setup-`. Every old
# installer would be kept for ever, silently, and each is tens of megabytes.
sweep km-video-tools-setup       "$VERSION" dist/setup/windows/*

# ...and the macOS one, `dist/setup/macos/km-video-tools-setup-<version>-<arch>.pkg`. A separate line
# rather than a glob over `dist/setup/*` because `sweep` takes a directory of items, and the cost of
# forgetting one is the same as it was above.
sweep km-video-tools-setup       "$VERSION" dist/setup/macos/*

echo "dist-clean: $REMOVED older staged item(s); kept $VERSION"
