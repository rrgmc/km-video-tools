#!/usr/bin/env bash
#
# Builds the macOS setup program: one Apple installer package carrying both programs.
#
#   tools/platform/macos/installer.sh              # stage both, build the package, test it
#   tools/platform/macos/installer.sh --no-build   # build from what is already staged
#   tools/platform/macos/installer.sh --install    # ...and then install it here, for real
#   tools/platform/macos/installer.sh -v           # watch the staging and the build
#
#   -> dist/setup/macos/km-video-tools-setup-<version>-<arch>.pkg
#
# **A `.pkg` rather than a `.dmg`.** A `.dmg` is a folder you drag from, which is what
# `dist/km-video-downloader/macos/...` and its zip already are; it has no components, no install
# step, and nowhere to put a command so that it can be typed. The two halves of this product go to
# two different places, which is the thing a disk image cannot express.
#
# **System-wide, where the Windows installer is per-user, and that is the same argument reaching the
# opposite answer.** On Windows everything this configures is per-user -- the PATH entry is
# HKCU\Environment -- so a machine-wide install would configure it for one account and put the files
# where every account can see them. On macOS the two destinations that mean anything are
# /Applications and /usr/local, both system-wide, and `~/Applications` is a folder most people do not
# know they have.
#
# **It gathers; it does not build.** The payload comes out of tools/dist/cmd.sh, which is also what
# builds the .app bundle -- so nothing about the bundle's Info.plist or either README is restated
# here.
#
# Prerequisite: nothing. pkgbuild, productbuild, pkgutil and xmllint are in the base system.

set -euo pipefail

cd "$(dirname "$0")/../../.."

. tools/dist/common.sh

RES=tools/platform/macos/pkg
PRODUCT=com.rrgmc.km-video-tools
BUILD=1
INSTALL=0
VERBOSE=0

# The one place the component split is written down. `downloader` is the .app and goes to
# /Applications; `fetch` is the command line and goes to /usr/local/km-video-tools, because
# /usr/local/bin is for symlinks and a package that owns a directory of its own can be removed by
# deleting that directory.
COMPONENTS=(downloader fetch)
install_location() { # <component>
  case "$1" in
    downloader) printf '/Applications' ;;
    fetch)      printf '/usr/local/km-video-tools' ;;
    *) echo "installer: no install location for $1" >&2; return 1 ;;
  esac
}

for arg in "$@"; do
  case "$arg" in
    --no-build) BUILD=0 ;;
    --install) INSTALL=1 ;;
    -v|--verbose) VERBOSE=1 ;;
    -h|--help)
      echo "usage: tools/platform/macos/installer.sh [--no-build] [--install] [-v]"
      exit 0 ;;
    *) echo "installer: unknown option $arg" >&2; exit 2 ;;
  esac
done

if [ "$(dist_platform)" != "macos" ]; then
  echo "installer: this builds a macOS installer package and has to run on macOS." >&2
  echo "           pkgbuild and productbuild are macOS programs; there is no cross-build." >&2
  echo "           The Windows carrier is tools/platform/windows/installer.sh." >&2
  exit 1
fi

# Resolved before anything is built, so a machine without the command line tools fails in a second
# with the line that fixes it rather than after a staging run.
for tool in pkgbuild productbuild pkgutil xmllint lipo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "installer: $tool is not on PATH." >&2
    echo "           xcode-select --install" >&2
    exit 1
  fi
done

echo "== macos installer"

# -- what to carry --------------------------------------------------------------------------------

if [ "$BUILD" -eq 1 ]; then
  echo "== staging both programs"
  if [ "$VERBOSE" -eq 1 ]; then
    tools/dist/cmd.sh
  else
    log="$(mktemp)"
    if ! tools/dist/cmd.sh > "$log" 2>&1; then
      cat "$log" >&2; rm -f "$log"
      echo "installer: staging failed; the log is above." >&2
      exit 1
    fi
    rm -f "$log"
  fi
fi

TRIPLE="$(dist_host_triple)"

staged_dir() { # <app>
  local app="$1" match
  for match in "$(dist_dir "$app" macos)/$app-"*"-$TRIPLE"; do
    [ -d "$match" ] && { printf '%s' "$match"; return 0; }
  done
  return 1
}

for app in km-video-downloader km-video-fetch; do
  if ! staged_dir "$app" >/dev/null; then
    echo "installer: nothing staged for $app under $(dist_dir "$app" macos)." >&2
    if [ "$BUILD" -eq 0 ]; then
      echo "           --no-build was given, so nothing was staged for it here either." >&2
    fi
    exit 1
  fi
done

DOWNLOADER_DIR="$(staged_dir km-video-downloader)"
FETCH_DIR="$(staged_dir km-video-fetch)"
BUNDLE="$DOWNLOADER_DIR/KM Video Downloader.app"

if [ ! -d "$BUNDLE" ]; then
  echo "installer: $DOWNLOADER_DIR has no .app bundle." >&2
  echo "           tools/dist/cmd.sh builds one only when icon/km-video-downloader.icns exists;" >&2
  echo "           run 'task icon' and stage again." >&2
  exit 1
fi

# From the binary, never the manifest -- the same rule the Windows driver states, for the same
# reason: asking the binary cannot disagree with the binary.
VERSION="$(dist_version "$FETCH_DIR/km-video-fetch")"

# `x86_64,arm64` when the binary is Intel, because Rosetta will run it; `arm64` alone when it is
# native, because an arm64 package must not offer itself to a machine that cannot run it.
case "$(lipo -archs "$FETCH_DIR/km-video-fetch" 2>/dev/null || echo unknown)" in
  *arm64*) ARCHS="arm64" ;;
  *)       ARCHS="x86_64,arm64" ;;
esac

OUTDIR="$(dist_dir setup macos)"
mkdir -p "$OUTDIR"
PKG="$OUTDIR/km-video-tools-setup-$VERSION-${TRIPLE%%-*}.pkg"
rm -f "$PKG"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/root/downloader" "$STAGE/root/fetch" "$STAGE/pkgs" "$STAGE/resources" "$STAGE/scripts/fetch"

# -- staging each component's root ----------------------------------------------------------------
#
# **`ditto` and never `cp -R`** for the bundle: a .app is made of symlinks and, once signed, a
# signature sealed over all of it, and the copy has to preserve every one or `codesign --verify`
# stops agreeing it will load elsewhere. The flat file takes the same call for consistency.
ditto "$BUNDLE" "$STAGE/root/downloader/KM Video Downloader.app"
ditto "$FETCH_DIR/km-video-fetch" "$STAGE/root/fetch/km-video-fetch"
cp "$FETCH_DIR/LICENSE-MIT" "$FETCH_DIR/LICENSE-APACHE" "$STAGE/root/fetch/"
dist_installed_readme macos > "$STAGE/root/fetch/README.txt"

# **pkgbuild silently drops every .DS_Store it finds, at any depth**, by its own default filter. That
# is the right behavior and the wrong thing to be surprised by: the archive would then hold something
# different from what was staged, and the round trip's file-list comparison would fail with no
# explanation. Removing them here means the two lists are the same list.
find "$STAGE/root" -name .DS_Store -delete

# -- the postinstall ------------------------------------------------------------------------------
#
# The only script in this package, and the reason /usr/local/km-video-tools is a directory of its own:
# the files go there, and one symlink puts the command on a PATH that already exists. There is no
# "add me to your PATH" tick anywhere in this installer and nothing edits a .zshrc.
#
# **Nothing in it may fail the install.** distribution.xml sets require-scripts="true", so a nonzero
# exit here is a failed install -- and by the time this runs both programs are already where they
# belong. A missing symlink is an inconvenience somebody fixes with one command; a failed install
# rolls back a copy that had already succeeded.
#
# `mkdir -p /usr/local/bin` and never `chown` or `chmod` on it: on an Intel Mac that directory is
# Homebrew's, and taking its ownership would break every formula installed there.
cat > "$STAGE/scripts/fetch/postinstall" <<'POSTINSTALL'
#!/bin/sh
# Put km-video-fetch where it can be typed. Best effort, always exits 0 -- see installer.sh.
mkdir -p /usr/local/bin 2>/dev/null || exit 0
ln -sfn /usr/local/km-video-tools/km-video-fetch /usr/local/bin/km-video-fetch 2>/dev/null || exit 0
exit 0
POSTINSTALL
chmod +x "$STAGE/scripts/fetch/postinstall"
# `sh -n` before it is packaged: a syntax error in here is a script that only fails on somebody
# else's Mac, halfway through their install.
sh -n "$STAGE/scripts/fetch/postinstall" \
  || { echo "installer: the generated postinstall is not valid shell." >&2; exit 1; }

# -- the component packages -----------------------------------------------------------------------
#
# One pkgbuild per component, because pkgbuild takes one --root and one --install-location: two
# destinations cannot share a package, which is the reason this is two and not one.
for comp in "${COMPONENTS[@]}"; do
  args=(--root "$STAGE/root/$comp"
        --identifier "$PRODUCT.$comp"
        --version "$VERSION"
        --install-location "$(install_location "$comp")"
        --ownership recommended)

  # The .app must not be relocated. Left to itself, Installer finds an older copy anywhere on the
  # disk -- including one in ~/Downloads that somebody unzipped once -- and updates *that* instead of
  # /Applications. `BundleIsRelocatable=false` is the only thing that stops it.
  if [ "$comp" = downloader ]; then
    pkgbuild --analyze --root "$STAGE/root/$comp" "$STAGE/downloader-component.plist" >/dev/null
    plutil -replace BundleIsRelocatable -bool NO "$STAGE/downloader-component.plist"
    plutil -replace BundleIsVersionChecked -bool NO "$STAGE/downloader-component.plist"
    args+=(--component-plist "$STAGE/downloader-component.plist")
  fi

  # The command line gets a postinstall, and it is the only script in this package.
  if [ "$comp" = fetch ]; then
    args+=(--scripts "$STAGE/scripts/fetch")
  fi

  pkgbuild "${args[@]}" "$STAGE/pkgs/km-video-tools-$comp.pkg" >/dev/null
done

# -- the Distribution -----------------------------------------------------------------------------

sed -e "s/@VERSION@/$VERSION/g" -e "s/@ARCHS@/$ARCHS/g" "$RES/distribution.xml" > "$STAGE/distribution.xml"
xmllint --noout "$STAGE/distribution.xml"

# The minimum is stated in the Distribution and nowhere else in this repository, so there is nothing
# to reconcile it against -- but a Distribution that lost the element entirely would install onto a
# system too old to open the window, silently. Asserted rather than assumed.
grep -q '<os-version min=' "$STAGE/distribution.xml" \
  || { echo "installer: $RES/distribution.xml no longer states a minimum OS version." >&2; exit 1; }

cp "$RES/welcome.html" "$RES/conclusion.html" "$STAGE/resources/"
cp LICENSE-MIT "$STAGE/resources/LICENSE-MIT"

# **Each pane must begin with a doctype, and this is a real fault rather than a style rule.**
# Installer sniffs the file's data to decide whether it is HTML or plain text, and one starting with
# a comment fails that sniff -- the pane is then shown as raw markup, authoring comment and all, and
# `mime-type="text/html"` in the Distribution does not rescue it.
for pane in welcome conclusion; do
  head -1 "$STAGE/resources/$pane.html" | grep -qi '^<!DOCTYPE' \
    || { echo "installer: $RES/$pane.html does not begin with a doctype." >&2
         echo "           Installer would show it as raw markup." >&2; exit 1; }
done

echo "== building the package"
productbuild --distribution "$STAGE/distribution.xml" \
             --package-path "$STAGE/pkgs" \
             --resources "$STAGE/resources" \
             "$PKG" >/dev/null

echo "dist: wrote $PKG"
echo "      version     $VERSION"
echo "      components  ${COMPONENTS[*]}"
echo "      archs       $ARCHS"
echo "      bytes       $(wc -c < "$PKG" | tr -d ' ')"

# -- the round trip -------------------------------------------------------------------------------
#
# Expanded rather than installed, because expanding needs no password and catches the failures that
# matter here: a component that carries the wrong files, an identifier that moved, an install
# location that is not what the choice said it was.
echo "== round trip"

EXPANDED="$STAGE/expanded"
pkgutil --expand-full "$PKG" "$EXPANDED"

for comp in "${COMPONENTS[@]}"; do
  inner="$EXPANDED/km-video-tools-$comp.pkg"
  [ -d "$inner" ] || { echo "installer: the package has no $comp component." >&2; exit 1; }

  grep -q "$PRODUCT.$comp" "$inner/PackageInfo" \
    || { echo "installer: $comp's PackageInfo does not carry $PRODUCT.$comp." >&2; exit 1; }
  grep -q "install-location=\"$(install_location "$comp")\"" "$inner/PackageInfo" \
    || { echo "installer: $comp's install-location is not $(install_location "$comp")." >&2; exit 1; }

  # Staged against archived, so a file dropped on the way into the package is caught here. This is
  # the macOS half of what the Windows driver gets by parsing [Files] back out of the .iss.
  ( cd "$STAGE/root/$comp" && find . -type f | sort ) > "$STAGE/staged-$comp.txt"
  ( cd "$inner/Payload"    && find . -type f | sort ) > "$STAGE/archived-$comp.txt"
  if ! diff -q "$STAGE/staged-$comp.txt" "$STAGE/archived-$comp.txt" >/dev/null; then
    echo "installer: $comp's archive does not hold what was staged:" >&2
    diff "$STAGE/staged-$comp.txt" "$STAGE/archived-$comp.txt" >&2 || true
    exit 1
  fi
done

# The negatives, which are the half a positive check cannot cover: neither component may carry the
# other's payload, or the choice on the Installer's page would be a choice about nothing.
[ -e "$EXPANDED/km-video-tools-fetch.pkg/Payload/KM Video Downloader.app" ] \
  && { echo "installer: the command-line component carries the application." >&2; exit 1; }
[ -e "$EXPANDED/km-video-tools-downloader.pkg/Payload/km-video-fetch" ] \
  && { echo "installer: the application component carries the command line." >&2; exit 1; }

# The application has to actually start. Run from the staged copy rather than the archive, with the
# dynamic-loader environment stripped, so an executable that silently depends on this shell's
# environment is caught here rather than on somebody else's Mac.
env -u DYLD_LIBRARY_PATH -u DYLD_FRAMEWORK_PATH -u DYLD_INSERT_LIBRARIES \
  "$STAGE/root/downloader/KM Video Downloader.app/Contents/MacOS/km-video-downloader" --version >/dev/null \
  || { echo "installer: the bundled km-video-downloader could not answer --version." >&2; exit 1; }
env -u DYLD_LIBRARY_PATH -u DYLD_FRAMEWORK_PATH -u DYLD_INSERT_LIBRARIES \
  "$STAGE/root/fetch/km-video-fetch" --version >/dev/null \
  || { echo "installer: the staged km-video-fetch could not answer --version." >&2; exit 1; }

echo "      both components expand to what was staged, and both programs run"

# -- and, if asked, the real thing ----------------------------------------------------------------
#
# Not part of the default run: it asks for an administrator password, writes to /Applications and
# /usr/local, and then takes it all back out again. That should be typed rather than set, which is
# why it is a flag with no variable behind it.
if [ "$INSTALL" -eq 1 ]; then
  echo "== installing for real (this asks for your password)"
  sudo installer -pkg "$PKG" -target /
  [ -d "/Applications/KM Video Downloader.app" ] \
    || { echo "installer: the application is not in /Applications." >&2; exit 1; }
  [ -x "/usr/local/bin/km-video-fetch" ] \
    || { echo "installer: /usr/local/bin/km-video-fetch is not there or not executable." >&2; exit 1; }
  /usr/local/bin/km-video-fetch --version
  echo "== removing it again"
  sudo rm -rf "/Applications/KM Video Downloader.app" /usr/local/km-video-tools /usr/local/bin/km-video-fetch
  sudo pkgutil --forget "$PRODUCT.downloader" >/dev/null 2>&1 || true
  sudo pkgutil --forget "$PRODUCT.fetch" >/dev/null 2>&1 || true
  echo "      installed, ran, removed"
fi
