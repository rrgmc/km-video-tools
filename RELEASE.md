# Releasing

A release is a tag, a GitHub release, and one setup program per platform attached to it. Everything
in it is built by hand on the platform it is for. Nothing here is automated, and the parts that
could not be automated are the reason: `.github/workflows/ci.yml` runs on pushes and pull requests
to `master`, never on a tag, and publishes nothing.

## The version is authored in one place

`[workspace.package] version` in `Cargo.toml`, and nothing else in the tree states it. The three
crates inherit it, clap turns it into `--version` for both programs, `winresource` writes it into
the Windows VERSIONINFO, and every script under `tools/dist` reads it back **out of the built
executable** rather than out of the manifest.

That last rule is what a release depends on: `dist_version()` in `tools/dist/common.sh` asks the
binary, so a folder name cannot disagree with the program inside it. `dist_staged_dir()` picks the
folder by the manifest and the binary inside still says what it is.

## Both artifacts are built on the machine they are for

Inno Setup is a Windows program and `pkgbuild` is a macOS one. Neither cross-builds, so a release
needs both machines and its two halves arrive at different times. **A tag is not a finished
release.** The second platform's package is attached afterwards, and the notes are edited then to
describe both.

## Cutting it

### 1. Bump the version, and write the changelog entry

One line in `Cargo.toml`, then any cargo command to update `Cargo.lock`. Commit as
`chore(release): X.Y.Z, <the phrase that names the release>`, with a body saying what moved and why
the minor rather than the patch.

The commit goes on a branch of its own and reaches `master` through a pull request, like every other
change. See `Nothing reaches master except through a pull request` in `docs/decisions.md`.

**`CHANGELOG.md` gains its entry in that same commit**, which is the one arrangement where the
number and the entry cannot disagree. It carries the date, the sections `Keep a Changelog` names,
and a link to the release, and that link is dead until step 5 publishes it. Keep it to what a reader
deciding whether to upgrade needs; the install steps and the checksums belong to the notes.

### 2. Prove the tree

```sh
task check          # toolchain pin, fmt, clippy -D warnings, tests
```

Run it on **both** platforms before tagging, and know that the numbers differ: two tests live behind
`#[cfg(all(test, target_os = "macos"))]` in `crates/km-video-downloader/src/alert.rs` and run
nowhere else. CI has no macOS runner, so a `task check` on a Mac is the only thing that compiles the
`#[cfg(target_os = "macos")]` code at all.

### 3. Stage and read the version back

```sh
task clean:old      # take away older staged versions -- see the trap below
task dist:setup     # or task dist:setup:notarized on macOS
```

Before tagging, confirm the number in every place it landed: the staged folder's name, both
binaries' `--version`, both staged `README.txt` files, and on Windows the executable's VERSIONINFO.
They are all derived from one build, so they agree or something is stale.

> **The trap `clean:old` exists for.** A staged folder carries its version in its name, so a build of
> one version lands *beside* another rather than replacing it. A carrier that searched for its
> payload would find the wrong one, and everything downstream would then agree about it, because the
> version is read out of those same executables. `dist_staged_dir()` names the folder instead of
> searching, and `clean:old` stops there being a second one to find. Run it first.

### 4. Merge, then tag

Open a pull request for the release branch and merge it once both CI jobs pass. Then tag the merge
commit on `master` with an annotated tag whose message is the bare version:

```sh
gh pr create --fill
gh pr merge --merge --delete-branch   # once CI passes
git switch master && git pull --ff-only origin master
git tag -a vX.Y.Z -m "X.Y.Z"
git push origin vX.Y.Z
```

The tag is pushed on its own because `master` accepts no push. The rule covers the branch only, so a
tag still goes straight to the remote.

### 5. Publish

```sh
gh release create vX.Y.Z \
  --title "vX.Y.Z — <the same phrase>" \
  --notes-file <notes> \
  dist/setup/<platform>/<the artifact>
```

### 6. Finish it from the other machine

```sh
git pull --ff-only origin master
git rev-parse HEAD vX.Y.Z^{commit}    # the same sha twice, and a clean tree
task check
task clean:old
task dist:setup:notarized             # on macOS; task dist:setup on Windows
gh release upload vX.Y.Z <the artifact>
gh release edit vX.Y.Z --notes-file <notes edited to describe both>
```

## The macOS package is signed, notarized and stapled

**`task dist:setup:notarized`, and nothing else is handed to anybody.** `task dist:setup` produces
`...-unsigned.pkg`, which is the right default for a build you are testing and the wrong file to
publish: a downloaded copy of it is refused by Gatekeeper, and telling people to right-click *Open*
is a workaround for a problem that has a fix.

What the task does, in `tools/platform/macos/installer.sh`:

- resolves both certificates out of the keychain — a Developer ID **Application** for code, a
  Developer ID **Installer** for the package, because `productbuild` refuses the other one — and
  checks the notary profile against Apple **before staging**, so a wrong value costs a second;
- **signs as it stages.** `dist_codesign()` in `tools/dist/common.sh` applies `--options runtime` at
  every call, because sealing a bundle does not add the hardened runtime to code already signed
  inside it, and `codesign --verify --deep --strict` does not notice when it is missing;
- signs the package, submits it with `notarytool submit --wait`, and **staples** the returned ticket
  into the file, without which a first open on a machine that cannot reach Apple is refused exactly
  as an unsigned package would be;
- proves all of it before calling the build done: every shipped executable carries the team
  identifier, and the finished package passes `pkgutil --check-signature`, `stapler validate` and
  `spctl -a -t install`.

Three traps, each of which is refused or asserted by the script rather than left to be remembered:

- **`--notarize` cannot be combined with `--no-build`.** Code is signed as it is staged, so skipping
  the staging would leave an ad-hoc payload inside a signed wrapper.
- **The identity has to be exported**, because the payload is signed by `tools/dist/cmd.sh` in a
  child process. The round trip's *every shipped executable carries …* line is what reads that
  child's work back.
- **A signed `productbuild` may raise a keychain dialog** the build then waits on indefinitely, with
  no output after `== building the package`. Answer it with *Always Allow*. A build blocked on an
  invisible dialog looks exactly like one that has hung.

Credentials are stored once per machine and never appear in the tree:

```sh
xcrun notarytool store-credentials km-video-tools --apple-id <apple-id> --team-id <team-id>
```

The password it asks for is an app-specific one from appleid.apple.com. Ask Apple whether it took —
`xcrun notarytool history --keychain-profile km-video-tools` — and not the keychain: `notarytool`
uses the data-protection keychain, which the `security` CLI cannot see into, so a local probe reports
a working profile missing.

The Windows installer is unsigned and there is no equivalent for it. That needs a purchased
certificate, and until there is one SmartScreen stops a first run.

## What the notes say

The shape, section by section: an opening paragraph on what changed and for whom; **Install on
Windows**; **Install on macOS**; **What you need** (yt-dlp and ffmpeg are not bundled); **What is new
in X.Y.Z**; and **Verification**.

Verification carries the SHA-256 of every attached installer, what tree each was built from, and what
each one's own round trip proved. Both installers test themselves on every build — the Windows one
installs into a scratch location and reads the `.kmvf` association back out of the registry with
`reg.exe`, the macOS one expands the package it just wrote and diffs both components against what was
staged — so there is something specific to say, and saying it is the point of the section.

Write the notes for somebody who has not read the commits.

## Checking a published release

```sh
gh release download vX.Y.Z -p '<the artifact>' -D /tmp/check
shasum -a 256 /tmp/check/<the artifact>            # matches the notes
spctl -a -vv -t install /tmp/check/<the .pkg>      # accepted, source=Notarized Developer ID
xcrun stapler validate /tmp/check/<the .pkg>       # the ticket travelled with the file
```

Check the copy GitHub serves rather than the one on the build machine. That is the file people get.
