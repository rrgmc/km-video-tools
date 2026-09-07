#!/usr/bin/env bash
#
# Builds the Windows setup program: one installer carrying both programs this repository makes.
#
#   tools/platform/windows/installer.sh              # stage both, compile the installer, test it
#   tools/platform/windows/installer.sh --no-build   # compile from what is already staged
#   tools/platform/windows/installer.sh -v           # watch the staging and the compile
#
#   -> dist/setup/windows/km-video-tools-setup-<version>-x86_64.exe
#
# **It gathers; it does not build.** The payload comes out of `tools/dist/cmd.sh`, so every fact
# about what a staged folder holds and what each README says stays there and none of it is restated
# here. This script runs that one, assembles the two folders into one, and hands it to Inno Setup.
#
# **The console twin is not installed.** `km-video-downloader-console.exe` exists so that a Windows
# folder gives somebody something to type; an installed build has a Start Menu entry and a PATH.
# It is subtracted by name below rather than by omission, and the count is reported.
#
# Prerequisite: Inno Setup 6. `winget install JRSoftware.InnoSetup`. Nothing else -- the payload is
# already built by the time this needs it.

set -euo pipefail

cd "$(dirname "$0")/../../.."

. tools/dist/common.sh

ISS="tools/platform/windows/installer.iss"
TARGET="x86_64"
BUILD=1
VERBOSE=0

for arg in "$@"; do
  case "$arg" in
    --no-build) BUILD=0 ;;
    -v|--verbose) VERBOSE=1 ;;
    -h|--help)
      echo "usage: tools/platform/windows/installer.sh [--no-build] [-v]"
      exit 0 ;;
    *) echo "installer: unknown option $arg" >&2; exit 2 ;;
  esac
done

if [ "$(dist_platform)" != "windows" ]; then
  echo "installer: this builds a Windows setup program and has to run on Windows." >&2
  echo "           The macOS carrier is tools/platform/macos/installer.sh." >&2
  exit 1
fi

# -- the compiler ---------------------------------------------------------------------------------
#
# Resolved before anything is built, so a missing install fails in a second with the line that fixes
# it rather than after a staging run. The fix is one command and nobody should have to go and find it.
#
# **Three places, and the per-user one is first for a reason.** `winget install JRSoftware.InnoSetup`
# installs into %LOCALAPPDATA%\Programs when it is run without elevation, which is the usual case.
# Looking only in Program Files finds nothing and reports it as "not installed", which is the wrong
# sentence entirely.
find_iscc() {
  local candidate local_appdata
  if command -v ISCC.exe >/dev/null 2>&1; then command -v ISCC.exe; return 0; fi
  if command -v iscc >/dev/null 2>&1; then command -v iscc; return 0; fi
  # %LOCALAPPDATA% arrives as a Windows path, so pasting it in front of a POSIX one gives
  # `C:\Users\...\Local/Programs/...`. Test operators tolerate that and it is a poor thing to print
  # in an error message, so it is converted where Git Bash can do it.
  local_appdata="${LOCALAPPDATA:-}"
  if [ -n "$local_appdata" ] && command -v cygpath >/dev/null 2>&1; then
    local_appdata="$(cygpath -u "$local_appdata")"
  fi
  # Spelled out rather than read from %ProgramFiles(x86)%: a variable whose name contains parentheses
  # cannot be referenced by bash's parameter syntax at all, so the literal is the only honest form.
  for candidate in \
    "$local_appdata/Programs/Inno Setup 6/ISCC.exe" \
    "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" \
    "/c/Program Files/Inno Setup 6/ISCC.exe"
  do
    if [ -f "$candidate" ]; then printf '%s' "$candidate"; return 0; fi
  done
  return 1
}

if ! ISCC="$(find_iscc)"; then
  echo "installer: Inno Setup 6 is not installed -- no ISCC.exe on PATH or in the usual places." >&2
  echo "           winget install JRSoftware.InnoSetup" >&2
  exit 1
fi

# The banner's first line is `Inno Setup <major> Command-Line Compiler`. Asserted rather than assumed:
# the .iss uses CreateDownloadPage and ArchitecturesAllowed=x64compatible, neither of which exists in 5.
#
# `|| true` is load-bearing under `set -o pipefail`: ISCC with no arguments prints its banner and its
# usage and then exits non-zero, so without it the version probe takes the whole script down before it
# has printed anything at all.
iscc_major="$("$ISCC" 2>&1 | sed -n '1s/^Inno Setup \([0-9]*\).*/\1/p' || true)"
if [ -z "$iscc_major" ] || [ "$iscc_major" -lt 6 ]; then
  echo "installer: $ISCC is not Inno Setup 6 or newer." >&2
  echo "           winget install JRSoftware.InnoSetup" >&2
  exit 1
fi

echo "== inno setup"
[ "$VERBOSE" -eq 1 ] && echo "   iscc  $ISCC (version $iscc_major)"

# -- what to carry --------------------------------------------------------------------------------
#
# **The payload is `dist/bin/windows`**, which tools/dist/bin.sh produces. That script already knows
# the two things this one would otherwise have to know a second time: that both staged folders carry
# a `README.txt` under the same name and one of them has to be renamed, and that a `-console` twin is
# the printing form of a pair rather than a second program. Restating either here would be two
# implementations of one rule, and the day they disagree is the day the installer ships the wrong
# README without anybody noticing.

if [ "$BUILD" -eq 1 ]; then
  echo "== staging every program"
  args=()
  [ "$VERBOSE" -eq 1 ] && args=(-v)
  if [ "$VERBOSE" -eq 1 ]; then
    tools/dist/bin.sh "${args[@]+"${args[@]}"}"
  else
    log="$(mktemp)"
    if ! tools/dist/bin.sh > "$log" 2>&1; then
      cat "$log" >&2
      rm -f "$log"
      echo "installer: staging failed; the log is above." >&2
      exit 1
    fi
    rm -f "$log"
  fi
fi

PAYLOAD="$(dist_dir bin windows)"

if [ ! -d "$PAYLOAD" ]; then
  echo "installer: nothing gathered at $PAYLOAD." >&2
  if [ "$BUILD" -eq 0 ]; then
    echo "           --no-build was given, so nothing was gathered for it here either. Run this" >&2
    echo "           script without it, or tools/dist/bin.sh first." >&2
  fi
  exit 1
fi

OUTDIR="$(dist_dir setup windows)"
GENDIR="$OUTDIR/generated"
mkdir -p "$OUTDIR"
dist_fresh_dir "$GENDIR"

# The installed build's document, in a directory of its own rather than in the payload -- it is not
# one of the gathered folder's files, and putting it there would make the reconciliation below a lie.
dist_installed_readme windows > "$GENDIR/README.txt"

# -- what the .iss says it installs, against what is actually there --------------------------------
#
# **Read out of the .iss rather than restated here.** The whole point is that the two cannot disagree,
# so this parses the [Files] section for its `Source:` paths under `{#Payload}` and expands them
# against the folder. Anything in the payload that no entry covers is a file that would be silently
# dropped from the installer, which is the one failure a carrier must not have.
covered="$(mktemp)"
present="$(mktemp)"
trap 'rm -f "$covered" "$present"' EXIT

while IFS= read -r spec; do
  [ -n "$spec" ] || continue
  for match in "$PAYLOAD"/$spec; do
    [ -e "$match" ] && printf '%s\n' "$match" >> "$covered"
  done
done < <(sed -n 's/^ *Source: *"{#Payload}\\\([^"]*\)".*/\1/p' "$ISS" | tr '\\' '/')

sort -u "$covered" -o "$covered"

# **`README.txt` is subtracted by name, and it is the one file left out on purpose rather than by
# omission.** It is the document tools/dist/bin.sh writes for `dist/bin/windows`, describing a folder
# somebody unpacked -- right about that folder and wrong about an installed build in most of its
# sentences. An installed build gets `dist_installed_readme`'s text from {#Generated} instead. The
# per-program `README-*.txt` are still installed, and are named in [Files] like everything else.
find "$PAYLOAD" -type f -not -path "$PAYLOAD/README.txt" | sort -u > "$present"

unaccounted="$(comm -23 "$present" "$covered" || true)"
if [ -n "$unaccounted" ]; then
  echo "installer: the payload holds files that $ISS does not install:" >&2
  printf '%s\n' "$unaccounted" | sed "s|^$PAYLOAD/|             |" >&2
  echo "           Add them to [Files] with the component that needs them, or stop tools/dist/bin.sh" >&2
  echo "           gathering them. Nothing is dropped from a carrier by accident." >&2
  exit 1
fi

# And the other direction: an entry naming a file that is not there compiles to an ISCC error a long
# way from its cause, so it is caught here where the sentence can name the section.
missing="$(comm -13 "$present" "$covered" || true)"
[ -z "$missing" ] || { echo "installer: [Files] names files the payload does not have:" >&2
                       printf '%s\n' "$missing" >&2; exit 1; }

# -- what the components are, read out of the .iss too ---------------------------------------------
#
# Parsed rather than restated, for the same reason. The round trip installs "everything" by naming
# the components, and a list written out in bash is one that stops agreeing the day a third program
# is added -- which would leave the round trip selecting nothing and proving nothing, a failure worse
# than a broken build because it passes.
COMPONENTS=()
while IFS= read -r component; do COMPONENTS+=("$component"); done < <(
  sed -n '/^\[Components\]/,/^\[/ s/^ *Name: *"\([^"]*\)".*/\1/p' "$ISS"
)
case " ${COMPONENTS[*]} " in
  *" downloader "*) ;;
  *) echo "installer: no [Components] found in $ISS; the round trip would verify nothing." >&2
     exit 1 ;;
esac

# -- the extension and the class it points at, read out of the .iss as well -----------------------
#
# Parsed for the third time and the third reason. These two are what the round trip below looks for
# in the registry, and a copy of them written out here would be a copy that can disagree -- with the
# failure landing on the *assertion* rather than on the installer, which is the worst way round: a
# correct build reported as broken teaches somebody to distrust the check.
iss_define() { # <name>
  sed -n "s/^#define *$1 *\"\([^\"]*\)\".*/\1/p" "$ISS" | head -n 1
}
EXTENSION="$(iss_define Extension)"
PROGID="$(iss_define ProgId)"
[ -n "$EXTENSION" ] && [ -n "$PROGID" ] \
  || { echo "installer: no Extension/ProgId defined in $ISS; the association cannot be checked." >&2
       exit 1; }

# -- the version ----------------------------------------------------------------------------------
#
# From the binary, never the manifest: every crate here says `version.workspace = true`, so reading
# it means picking the right one of several `version =` lines, and asking the binary cannot disagree
# with the binary.
#
# **Deliberately the GUI-subsystem executable**, though the payload has no console twin left to ask
# instead. Standard handles are inherited whatever the subsystem, so a GUI-subsystem process answers
# `--version` perfectly well down the pipe dist_version puts it on.
VERSION="$(dist_version "$PAYLOAD/km-video-downloader.exe")"
OUTBASE="km-video-tools-setup-$VERSION-$TARGET"
rm -f "$OUTDIR/$OUTBASE.exe"

# -- the compile ----------------------------------------------------------------------------------
#
# MSYS2_ARG_CONV_EXCL is not optional and not cosmetic. Git Bash rewrites any argument that looks
# like a POSIX path, so `/DPayload=...` arrives at a Windows program as
# `C:/Program Files/Git/DPayload=...` -- the same trap that makes `makensis /VERSION` report that it
# cannot open a script called VERSION. Every value is additionally converted to a Windows path,
# because ISCC is a Windows program and has never heard of /c/prog.
echo "== compiling the installer"
iscc_args=(
  "/DPayload=$(host_path "$PWD/$PAYLOAD")"
  "/DGenerated=$(host_path "$PWD/$GENDIR")"
  "/DVersion=$VERSION"
  "/DOutDir=$(host_path "$PWD/$OUTDIR")"
  "/DOutBase=$OUTBASE"
  "$(host_path "$PWD/$ISS")"
)
if [ "$VERBOSE" -eq 1 ]; then
  MSYS2_ARG_CONV_EXCL='*' "$ISCC" "${iscc_args[@]}"
else
  log="$(mktemp)"
  if ! MSYS2_ARG_CONV_EXCL='*' "$ISCC" "${iscc_args[@]}" > "$log" 2>&1; then
    cat "$log" >&2
    rm -f "$log"
    exit 1
  fi
  rm -f "$log"
fi

SETUP="$OUTDIR/$OUTBASE.exe"
[ -f "$SETUP" ] || { echo "installer: ISCC reported success but $SETUP is not there." >&2; exit 1; }

echo "dist: wrote $SETUP"
echo "      version     $VERSION"
echo "      components  ${COMPONENTS[*]}"
echo "      bytes       $(wc -c < "$SETUP" | tr -d ' ')"

# -- the round trip -------------------------------------------------------------------------------
#
# Install it silently into a scratch directory, prove every executable is there and runs, then
# uninstall and prove the directory is gone. A setup program that has never been installed once is a
# file nobody has tested, and the failure it hides -- a missing file, an exe that cannot start --
# looks exactly like a successful build until somebody else finds it.
#
# **Cleanup runs the uninstaller rather than `rm -rf`.** `/DIR=` redirects `{app}` and nothing else,
# so the Start Menu group and the HKCU uninstall row are real; deleting the folder would orphan the
# row and leave a broken entry in Add or remove programs.
SCRATCH="$(mktemp -d)"
cleanup_scratch() {
  if [ -f "$SCRATCH/app/unins000.exe" ]; then
    MSYS2_ARG_CONV_EXCL='*' "$SCRATCH/app/unins000.exe" /VERYSILENT /NORESTART >/dev/null 2>&1 || true
    sleep 2
  fi
  rm -rf "$SCRATCH"
}
trap 'rm -f "$covered" "$present"; cleanup_scratch' EXIT

echo "== round trip"

components="$(IFS=,; printf '%s' "${COMPONENTS[*]}")"
# **One task of the three, and choosing which is the point.**
#
# `addpath` and `desktopicon` stay off: a build machine's PATH is not this script's to grow, and a
# desktop this script did not put anything on is one it does not have to tidy.
#
# `associate` is on, and it is the exception on purpose. It writes real keys under HKCU -- outside
# the scratch directory, like the Start Menu group and the uninstall row already are -- and *that
# is what needs proving*. An association nobody has ever installed is an association nobody has
# ever removed either, and a setup program leaving a dead handler behind after an uninstall is
# exactly the failure this round trip exists to find. Both halves are asserted below.
MSYS2_ARG_CONV_EXCL='*' "./$SETUP" /VERYSILENT /SP- /NORESTART \
  /TASKS="associate" "/COMPONENTS=$components" "/DIR=$(host_path "$SCRATCH/app")"

# Inno's Setup.exe extracts itself and hands the work to a second process, so the first one returning
# does not mean the install has finished. The uninstaller appearing is what does.
for _ in $(seq 1 60); do
  [ -f "$SCRATCH/app/unins000.exe" ] && break
  sleep 1
done
[ -f "$SCRATCH/app/unins000.exe" ] || { echo "installer: the install never produced an uninstaller." >&2; exit 1; }

# On a bare PATH, so that an executable which silently depends on something in this shell's
# environment is caught here rather than on somebody else's machine.
for exe in km-video-downloader.exe km-video-fetch.exe; do
  [ -f "$SCRATCH/app/$exe" ] || { echo "installer: $exe was not installed." >&2; exit 1; }
  PATH="/c/Windows/System32:/c/Windows" "$SCRATCH/app/$exe" --version >/dev/null \
    || { echo "installer: the installed $exe could not answer --version." >&2; exit 1; }
done

# The negative, which is the half a positive check cannot cover: the console twin must not be there.
for twin in "$SCRATCH"/app/*-console.exe; do
  [ -e "$twin" ] && { echo "installer: a console twin was installed: $twin" >&2; exit 1; }
done

# And the installed README must be the installed one rather than a folder's. Checked from both
# sides, because either mistake is silent: the sentence that only an installed build can say must be
# there, and the heading only a staged folder writes must not.
grep -q "This was installed for your account only" "$SCRATCH/app/README.txt" \
  || { echo "installer: the installed README.txt is not dist_installed_readme's." >&2; exit 1; }
grep -q "^km-video-downloader $VERSION" "$SCRATCH/app/README.txt" \
  && { echo "installer: a staged folder's README.txt was installed instead." >&2; exit 1; }

# The association, which is the one thing this install put outside the scratch directory.
#
# **Read back with `reg.exe` rather than trusted.** Inno reports nothing about a [Registry] entry it
# skipped, so a mistyped `Tasks:` or a `Components:` that never matches produces a successful build
# and an installer that quietly associates nothing.
reg_default() { # <key> -> its default value, or nothing
  MSYS2_ARG_CONV_EXCL='*' reg.exe query "$1" /ve 2>/dev/null \
    | sed -n 's/.*REG_SZ[[:space:]]*//p' | tr -d '\r'
}
kmvf_class() { # -> the ProgId the extension points at, or nothing
  reg_default "HKCU\\Software\\Classes\\$EXTENSION"
}
kmvf_command() { # -> the command line Windows would run for one
  reg_default "HKCU\\Software\\Classes\\$PROGID\\shell\\open\\command"
}
# The Open with registration, which is a *key* rather than a value and so has to be asked for
# differently. It caught a real leftover: `uninsdeletekey` on the SupportedTypes key below it
# removed that and stopped, leaving an empty `Applications\km-video-downloader.exe` behind -- a
# program with nothing on the machine still listed in the registry.
kmvf_openwith() { # -> the key's own path, or nothing
  MSYS2_ARG_CONV_EXCL='*' \
    reg.exe query 'HKCU\Software\Classes\Applications\km-video-downloader.exe' 2>/dev/null \
    | sed -n 's/^HKEY.*/present/p' | head -n 1
}

[ "$(kmvf_class)" = "$PROGID" ] \
  || { echo "installer: $EXTENSION was not associated; it points at '$(kmvf_class)'." >&2; exit 1; }

# The path must be quoted, or Windows hands over only what precedes the first space -- and the
# scratch directory this ran in is the shape that would still pass unquoted.
case "$(kmvf_command)" in
  '"'*'km-video-downloader.exe" "%1"') ;;
  *) echo "installer: the open command is not a quoted exe and a quoted %1: $(kmvf_command)" >&2
     exit 1 ;;
esac

[ "$(kmvf_openwith)" = present ] \
  || { echo "installer: km-video-downloader was not offered in Open with." >&2; exit 1; }

MSYS2_ARG_CONV_EXCL='*' "$SCRATCH/app/unins000.exe" /VERYSILENT /NORESTART || true
# The uninstaller relaunches itself from a temp copy, so the process exiting is not the end of it.
for _ in $(seq 1 60); do
  [ -d "$SCRATCH/app" ] || break
  sleep 1
done
[ -d "$SCRATCH/app" ] && { echo "installer: the uninstaller left $SCRATCH/app behind." >&2; exit 1; }

# The other half, and the one an uninstaller is likeliest to get wrong: a list on this machine must
# no longer open a program that is no longer here.
[ -z "$(kmvf_class)" ] \
  || { echo "installer: the uninstaller left $EXTENSION pointing at '$(kmvf_class)'." >&2; exit 1; }
[ -z "$(kmvf_command)" ] \
  || { echo "installer: the uninstaller left the ProgId behind: $(kmvf_command)" >&2; exit 1; }
[ -z "$(kmvf_openwith)" ] \
  || { echo "installer: the uninstaller left the Open with registration behind." >&2; exit 1; }

echo "      installed both programs, ran each, associated $EXTENSION, uninstalled, nothing left"
