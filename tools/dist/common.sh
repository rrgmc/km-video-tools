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

# The one place the layout is written down:
#
#   dist/<app>/<platform>/<app>-<version>-<triple>/
#
# The folder carries the version and the triple because it is a thing you hand to somebody, and what
# they will ask first is which build they were given. `dist/` itself is git-ignored — it is output.
dist_dir() { # <app> <platform>
  printf 'dist/%s/%s' "$1" "$2"
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
