#!/usr/bin/env bash
#
# Builds the macOS setup program: one Apple installer package carrying both programs.
#
#   tools/platform/macos/installer.sh              # stage both, build the package, test it
#   tools/platform/macos/installer.sh --notarize   # ...signed, sent to Apple and stapled
#   tools/platform/macos/installer.sh --no-build   # build from what is already staged
#   tools/platform/macos/installer.sh --install    # ...and then install it here, for real
#   tools/platform/macos/installer.sh -v           # watch the staging and the build
#
#   -> dist/setup/macos/km-video-tools-setup-<version>-<arch>.pkg              --notarize
#      ...-<arch>-unnotarized.pkg   signed only, which spctl still refuses
#      ...-<arch>-unsigned.pkg      ad-hoc, which is the default
#
# **Three names rather than one, because the three are not interchangeable and a shared name loses
# the good one.** An ordinary run minutes after a notarized one would otherwise replace a package
# that had been to Apple with an ad-hoc file of the same name and no visible difference. Only the
# notarized build gets the plain name, because it is the only one worth handing over.
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
NOTARIZE=0

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
    --notarize) NOTARIZE=1 ;;
    -v|--verbose) VERBOSE=1 ;;
    -h|--help)
      echo "usage: tools/platform/macos/installer.sh [--notarize] [--no-build] [--install] [-v]"
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

# -- what signs it --------------------------------------------------------------------------------
#
# **Two certificates, because they are two jobs.** A Developer ID *Application* signs Mach-O code and
# bundles and is what `dist_codesign` uses at staging time; a Developer ID *Installer* signs the
# `.pkg` itself, and `productbuild` refuses the other one. Naming a single "signing identity" is the
# mistake this pair exists to prevent.
#
# **Discovered rather than written down.** Both are read out of the keychain, so no tracked file
# names a certificate holder and a checkout works unchanged on any machine that has its own. Either
# can still be given explicitly, which is the only way to sign as somebody else:
#
#     KM_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" \
#     KM_SIGN_INSTALLER_IDENTITY="Developer ID Installer: Name (TEAMID)" \
#     KM_NOTARY_PROFILE=km-video-tools tools/platform/macos/installer.sh --notarize
#
# `KM_NOTARY_PROFILE` is only the *label* of credentials stored by `xcrun notarytool
# store-credentials`; the Apple ID and password behind it never leave the data-protection keychain,
# which is why a name is all this repository ever holds.
KM_NOTARY_PROFILE="${KM_NOTARY_PROFILE:-km-video-tools}"
KM_SIGN_INSTALLER_IDENTITY="${KM_SIGN_INSTALLER_IDENTITY:-}"

# Exactly one, or say which. Zero is a machine without the certificate; several is a choice this
# script must not make silently, because the wrong one produces a package Apple accepts and the
# wrong team ships.
sole_identity() { # <certificate kind> -> prints the identity, or fails
  local kind="$1" found count
  found="$(security find-identity -v 2>/dev/null \
           | sed -n "s/.*\"\($kind: .*\)\".*/\1/p")"
  count="$(printf '%s' "$found" | grep -c . || true)"
  if [ "$count" -eq 1 ]; then
    printf '%s' "$found"
    return 0
  fi
  if [ "$count" -eq 0 ]; then
    echo "installer: this keychain has no \"$kind\" certificate." >&2
    echo "           A Developer ID pair comes from an Apple Developer Program membership;" >&2
    echo "           download both from developer.apple.com and open them once." >&2
  else
    echo "installer: this keychain has $count \"$kind\" certificates and nothing here may pick one:" >&2
    printf '             %s\n' $found >&2
    echo "           Name the one you mean in KM_SIGN_IDENTITY / KM_SIGN_INSTALLER_IDENTITY." >&2
  fi
  return 1
}

identity_exists() { # <identity>
  security find-identity -v 2>/dev/null | grep -qF "\"$1\""
}

if [ "$NOTARIZE" -eq 1 ]; then
  # **Signing happens while the payload is staged, not when it is wrapped.** `--no-build` would leave
  # ad-hoc code inside a signed package, which only the round trip notices and Apple refuses. Refused
  # here so the cost is a second rather than a submission.
  if [ "$BUILD" -eq 0 ]; then
    echo "installer: --notarize cannot be combined with --no-build." >&2
    echo "           Code is signed as it is staged, so skipping the staging leaves an ad-hoc" >&2
    echo "           payload inside a signed wrapper." >&2
    exit 1
  fi

  for tool in codesign security; do
    command -v "$tool" >/dev/null 2>&1 \
      || { echo "installer: $tool is not on PATH." >&2; exit 1; }
  done
  xcrun --find notarytool >/dev/null 2>&1 \
    || { echo "installer: notarytool is not available." >&2
         echo "           It ships with the command line tools: xcode-select --install" >&2
         exit 1; }

  # Exported, because tools/dist/cmd.sh signs the payload in a child process and an unexported
  # assignment would never reach it -- leaving an ad-hoc bundle inside a signed package.
  if [ -z "${KM_SIGN_IDENTITY:-}" ]; then
    KM_SIGN_IDENTITY="$(sole_identity "Developer ID Application")" || exit 1
  fi
  export KM_SIGN_IDENTITY
  if [ -z "${KM_SIGN_INSTALLER_IDENTITY:-}" ]; then
    KM_SIGN_INSTALLER_IDENTITY="$(sole_identity "Developer ID Installer")" || exit 1
  fi

  for pair in "KM_SIGN_IDENTITY:$KM_SIGN_IDENTITY" "KM_SIGN_INSTALLER_IDENTITY:$KM_SIGN_INSTALLER_IDENTITY"; do
    if ! identity_exists "${pair#*:}"; then
      echo "installer: ${pair%%:*} names an identity this keychain does not have:" >&2
      echo "             ${pair#*:}" >&2
      echo "           security find-identity -v" >&2
      exit 1
    fi
  done

  # Asked of Apple rather than of the keychain. `notarytool` keeps its credentials in the
  # data-protection keychain, which the legacy `security` CLI cannot see into, so a local probe
  # reports a working profile missing.
  if ! xcrun notarytool history --keychain-profile "$KM_NOTARY_PROFILE" >/dev/null 2>&1; then
    echo "installer: no notarytool credentials are stored under \"$KM_NOTARY_PROFILE\"." >&2
    echo "           xcrun notarytool store-credentials $KM_NOTARY_PROFILE \\" >&2
    echo "             --apple-id <apple-id> --team-id $(printf '%s' "$KM_SIGN_IDENTITY" | sed -n 's/.*(\(.*\))/\1/p')" >&2
    echo "           The password it asks for is an app-specific one from appleid.apple.com." >&2
    exit 1
  fi
elif [ -n "${KM_SIGN_IDENTITY:-}" ]; then
  # Signing without notarizing is a real state and a poor one to ship: Gatekeeper refuses a
  # downloaded copy just the same. Allowed, named `-unnotarized`, and left to the caller.
  export KM_SIGN_IDENTITY
  KM_SIGN_INSTALLER_IDENTITY="${KM_SIGN_INSTALLER_IDENTITY:-$(sole_identity "Developer ID Installer")}" || exit 1
fi

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

for app in km-video-downloader km-video-fetch; do
  if ! dist_staged_dir "$app" macos >/dev/null; then
    echo "installer: the current version is not staged for $app; looked for" >&2
    echo "           $(dist_dir "$app" macos)/$app-$(dist_pkg_version)-$TRIPLE" >&2
    if [ "$BUILD" -eq 0 ]; then
      echo "           --no-build was given, so nothing was staged for it here either." >&2
    fi
    exit 1
  fi
done

DOWNLOADER_DIR="$(dist_staged_dir km-video-downloader macos)"
FETCH_DIR="$(dist_staged_dir km-video-fetch macos)"
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

# The suffix says what the file is, so that three builds of one version can sit in one folder and
# none of them can be mistaken for another. See the note at the top of this script.
if [ "$NOTARIZE" -eq 1 ]; then
  SUFFIX=""
elif dist_signing; then
  SUFFIX="-unnotarized"
else
  SUFFIX="-unsigned"
fi
PKG="$OUTDIR/km-video-tools-setup-$VERSION-${TRIPLE%%-*}$SUFFIX.pkg"
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

# The minimum has two homes -- here, and `LSMinimumSystemVersion` in the bundle, which is the only
# copy a `.app` handed over as a folder carries. So this reads the number out rather than merely
# proving the element is still there, and the round trip below fails if the two have drifted apart.
#
# A Distribution that lost the element entirely would install onto a system too old to open the
# window, silently. That is still what the emptiness check is for.
PKG_MIN="$(sed -n 's/.*<os-version min="\([^"]*\)".*/\1/p' "$STAGE/distribution.xml")"
[ -n "$PKG_MIN" ] \
  || { echo "installer: $RES/distribution.xml no longer states a minimum OS version." >&2; exit 1; }

cp "$RES/conclusion.html" "$STAGE/resources/"

# **The welcome pane tells somebody how to get past Gatekeeper, and a notarized package must not.**
# The advice is right for an ad-hoc build and wrong for this one -- there is no dialog to get past,
# and printing the workaround anyway teaches a habit that defeats the point of signing. The block is
# delimited in the file rather than kept as a second copy of the pane, so the two cannot drift.
#
# Asserted rather than assumed: a marker renamed in the pane would otherwise stop stripping anything
# and nothing would say so, which is the same silent-success failure the association checks exist for.
grep -q '^<!-- unsigned-only -->$' "$RES/welcome.html" \
  || { echo "installer: $RES/welcome.html has no <!-- unsigned-only --> marker." >&2
       echo "           The Gatekeeper paragraph is stripped from a signed build by that marker." >&2
       exit 1; }
if dist_signing; then
  sed '/^<!-- unsigned-only -->$/,/^<!-- \/unsigned-only -->$/d' "$RES/welcome.html" \
    > "$STAGE/resources/welcome.html"
  grep -q "not signed with a Developer ID" "$STAGE/resources/welcome.html" \
    && { echo "installer: the signed build's welcome pane still says it is unsigned." >&2; exit 1; }
else
  cp "$RES/welcome.html" "$STAGE/resources/welcome.html"
fi
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
productbuild_args=(--distribution "$STAGE/distribution.xml"
                   --package-path "$STAGE/pkgs"
                   --resources "$STAGE/resources")

# **The Installer certificate, not the Application one**, and `--timestamp` because a signature Apple
# is asked to notarize must carry a trusted timestamp. A signed `productbuild` against a key it has
# not been asked about before raises a keychain dialog that the build then waits on indefinitely,
# with no output after this line -- answer it with Always Allow.
if [ -n "$KM_SIGN_INSTALLER_IDENTITY" ]; then
  productbuild_args+=(--sign "$KM_SIGN_INSTALLER_IDENTITY" --timestamp)
fi

productbuild "${productbuild_args[@]}" "$PKG" >/dev/null

echo "dist: wrote $PKG"
echo "      version     $VERSION"
echo "      components  ${COMPONENTS[*]}"
echo "      archs       $ARCHS"
echo "      code        $(dist_signing_note)"
if [ -n "$KM_SIGN_INSTALLER_IDENTITY" ]; then
  echo "      installer   $KM_SIGN_INSTALLER_IDENTITY"
fi
echo "      bytes       $(wc -c < "$PKG" | tr -d ' ')"

# -- Apple ----------------------------------------------------------------------------------------
#
# **One submission covers everything**: notarytool looks inside the package at the nested code, so
# the `.pkg` is the only thing sent and the bundle inside it needs no submission of its own.
#
# **Stapled afterwards, and that is not optional for a file people download.** The notarization lives
# on Apple's servers until `stapler` writes the ticket into the package; without it a first open on a
# machine that cannot reach Apple is refused exactly as an unsigned package would be.
if [ "$NOTARIZE" -eq 1 ]; then
  echo "== notarizing (this waits on Apple)"
  if ! xcrun notarytool submit "$PKG" --keychain-profile "$KM_NOTARY_PROFILE" --wait; then
    echo "installer: Apple did not accept the package." >&2
    echo "           The submission id is above; what it objected to is in:" >&2
    echo "             xcrun notarytool log <submission-id> --keychain-profile $KM_NOTARY_PROFILE" >&2
    exit 1
  fi
  xcrun stapler staple "$PKG"
fi

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

# -- what the bundle says it opens ----------------------------------------------------------------
#
# **The macOS half of the association, read back out of the package.** The Windows driver reads its
# [Registry] entries with `reg.exe` for a reason that applies here word for word: nothing about
# building fails when the declaration is missing. `documents()` in tools/dist/cmd.sh writes these
# keys through a heredoc nested in another heredoc, so a mistake there produces a clean build, a
# package somebody installs, and an application LaunchServices files under nothing at all.
#
# **From the Payload rather than from `$STAGE/root`**, because what is being proved is what somebody
# receives. A staged bundle that is right beside an archive that is not is precisely the failure a
# round trip exists for.
PLIST="$EXPANDED/km-video-tools-downloader.pkg/Payload/KM Video Downloader.app/Contents/Info.plist"

[ -f "$PLIST" ] \
  || { echo "installer: the archived application has no Info.plist." >&2; exit 1; }
plutil -lint "$PLIST" >/dev/null \
  || { echo "installer: the bundle's Info.plist is not a valid plist." >&2
       echo "           tools/dist/cmd.sh writes it; something there produced markup Apple refuses." >&2
       exit 1; }

# `-extract <path> raw` prints a scalar's value and an array's *count*, which is why the extension is
# asked for twice below: once as the array, to say there is exactly one of them, and once by index,
# for what it is. A missing key exits non-zero and prints to stderr, so the empty string is a real
# answer meaning "not there" rather than an error being swallowed.
plist_value() { # <key path> -> prints the value, or nothing
  plutil -extract "$1" raw -o - "$PLIST" 2>/dev/null
}

DECLARED_UTI="$(plist_value UTExportedTypeDeclarations.0.UTTypeIdentifier)"
[ -n "$DECLARED_UTI" ] \
  || { echo "installer: the bundle exports no type declaration, so it associates nothing." >&2; exit 1; }

# **A document type naming a type the bundle does not declare** is the macOS shape of an extension
# registered to a ProgId with no command behind it: every piece present, and nothing opens.
CLAIMED_UTI="$(plist_value CFBundleDocumentTypes.0.LSItemContentTypes.0)"
[ "$CLAIMED_UTI" = "$DECLARED_UTI" ] \
  || { echo "installer: the document type opens '$CLAIMED_UTI' but the bundle declares '$DECLARED_UTI'." >&2
       exit 1; }

EXT_KEY='UTExportedTypeDeclarations.0.UTTypeTagSpecification.public\.filename-extension'
[ "$(plist_value "$EXT_KEY")" = 1 ] \
  || { echo "installer: the declared type does not carry exactly one filename extension." >&2; exit 1; }

# **Read rather than repeated.** There is one place the extension is decided for this platform --
# `documents()` -- and a copy typed here would agree with it right up until the day it did not. The
# Windows driver reads its own out of the .iss with `iss_define Extension` for the same reason.
EXTENSION="$(plist_value "$EXT_KEY.0")"
[ -n "$EXTENSION" ] \
  || { echo "installer: the declared type names no filename extension." >&2; exit 1; }

# Both of these are claims rather than descriptions, and both are load-bearing: `Owner` is this
# program saying it defines the type, and an identifier is what LaunchServices files the declaration
# under. Without the second the type is registered to nobody.
RANK="$(plist_value CFBundleDocumentTypes.0.LSHandlerRank)"
[ "$RANK" = Owner ] \
  || { echo "installer: the document type's LSHandlerRank is '$RANK' rather than Owner." >&2; exit 1; }
[ -n "$(plist_value CFBundleIdentifier)" ] \
  || { echo "installer: the bundle has no CFBundleIdentifier; the type would register to nobody." >&2
       exit 1; }

# The other half of the number the Distribution states above. Two files say the floor -- an XML
# attribute productbuild reads, and a plist key LaunchServices reads -- and neither can be derived
# from the other, so what is left is to refuse a build where they disagree.
BUNDLE_MIN="$(plist_value LSMinimumSystemVersion)"
[ "$BUNDLE_MIN" = "$PKG_MIN" ] \
  || { echo "installer: the Distribution requires macOS $PKG_MIN and the bundle says '$BUNDLE_MIN'." >&2
       echo "           tools/dist/common.sh's dist_min_macos writes the bundle's; make them one number." >&2
       exit 1; }

echo "      both components expand to what was staged, and both programs run"
echo "      the application declares $DECLARED_UTI, opens .$EXTENSION, and asks for macOS $BUNDLE_MIN"

# -- what a recipient's Mac will make of it --------------------------------------------------------
#
# **Asked of the payload rather than of the wrapper.** A package signed over ad-hoc code passes every
# check above and is refused by Apple, so what is verified here is each Mach-O that actually ships --
# which is also the only thing that proves `KM_SIGN_IDENTITY` reached the child process that staged
# it.
if dist_signing; then
  TEAM="$(dist_team_id)"
  [ -n "$TEAM" ] \
    || { echo "installer: KM_SIGN_IDENTITY carries no team identifier in parentheses." >&2; exit 1; }

  # **Captured and then matched, never `codesign | grep -q`.** `grep -q` exits on the first match and
  # `set -o pipefail` then reports the SIGPIPE that gives the producer, so the pipeline fails on
  # exactly the runs where the answer was yes.
  signed_count=0
  while IFS= read -r macho; do
    [ -n "$macho" ] || continue
    seal="$(codesign -dv --verbose=2 "$macho" 2>&1 || true)"
    if ! grep -q "^TeamIdentifier=$TEAM$" <<<"$seal"; then
      echo "installer: $macho is not signed by $TEAM, so the submission would be refused." >&2
      echo "           tools/dist/cmd.sh signs the payload in a child process; KM_SIGN_IDENTITY" >&2
      echo "           has to be exported to reach it." >&2
      exit 1
    fi
    signed_count=$((signed_count + 1))
  done <<MACHOS
$STAGE/root/downloader/KM Video Downloader.app/Contents/MacOS/km-video-downloader
$STAGE/root/downloader/KM Video Downloader.app
$STAGE/root/fetch/km-video-fetch
MACHOS

  codesign --verify --deep --strict "$STAGE/root/downloader/KM Video Downloader.app" \
    || { echo "installer: the application bundle's signature does not verify." >&2; exit 1; }

  echo "      every shipped executable carries $TEAM ($signed_count of them)"
fi

# `spctl` is the question a recipient's Mac asks, and it is the only one of these three that a
# signed-but-not-notarized package fails. Run for the notarized build alone, because that is the
# only build claiming to pass it.
if [ "$NOTARIZE" -eq 1 ]; then
  signature="$(pkgutil --check-signature "$PKG" 2>&1 || true)"
  # `pkgutil` and `spctl` word this differently and only one of them is being asked here: the phrase
  # below is `pkgutil`'s, and `source=Notarized Developer ID` is `spctl`'s, checked further down.
  grep -q "^   Notarization: trusted by the Apple notary service$" <<<"$signature" \
    || { echo "installer: pkgutil does not call this a notarized package:" >&2
         printf '%s\n' "$signature" >&2; exit 1; }
  xcrun stapler validate "$PKG" >/dev/null 2>&1 \
    || { echo "installer: the notarization ticket is not stapled to the package." >&2
         echo "           A first open with no network would be refused." >&2; exit 1; }
  spctl -a -vvv -t install "$PKG" >/dev/null 2>&1 \
    || { echo "installer: spctl refuses the .pkg, so a recipient's Mac would too." >&2; exit 1; }
  echo "      signed, notarized, stapled, and accepted by spctl"
fi

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
