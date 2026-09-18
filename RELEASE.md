# Releasing

A release is a tag, a GitHub release, and one setup program per platform attached to it. Pushing the
tag builds the Windows installer on a runner and attaches it to a draft release; the macOS package
is built, signed and notarized on a Mac and attached by hand. See `The Windows installer is built by
the tag, and the macOS package by hand` in `docs/decisions.md`.

`.github/workflows/ci.yml` runs on pushes and pull requests to `master` and publishes nothing.
`.github/workflows/release.yml` runs on a `v*` tag, and on a manual button that stops at a workflow
artifact. That button is the dry run, and GitHub offers it for workflows on the default branch
alone, so the release job is proved from `master` rather than from the branch that adds it.

## The version is authored in one place

`[workspace.package] version` in `Cargo.toml`, and nothing else in the tree states it. The three
crates inherit it, clap turns it into `--version` for both programs, `winresource` writes it into
the Windows VERSIONINFO, and every script under `tools/dist` reads it back **out of the built
executable** rather than out of the manifest.

That last rule is what a release depends on: `dist_version()` in `tools/dist/common.sh` asks the
binary, so a folder name cannot disagree with the program inside it. `dist_staged_dir()` picks the
folder by the manifest and the binary inside still says what it is. The tag build asks the same
question of the tag: `vX.Y.Z` has to equal what the executable answers, or the job attaches nothing.

## The two halves arrive at different times

Inno Setup is a Windows program and `pkgbuild` is a macOS one. Neither cross-builds, so a release
needs both platforms and only one of them is a runner. **A tag is not a finished release.** The
macOS package is attached afterwards and the notes are written then to describe both, which is why
the tag leaves a draft.

## Cutting it

### 1. Bump the version, and write the changelog entry

One line in `Cargo.toml`, then any cargo command to update `Cargo.lock`. Commit as
`chore(release): X.Y.Z, <the phrase that names the release>`, with a body saying what moved and why
the minor rather than the patch.

The commit goes on a branch of its own and reaches `master` through a pull request, like every other
change. See `Nothing reaches master except through a pull request` in `docs/decisions.md`.

**`CHANGELOG.md` gains its entry in that same commit**, which is the one arrangement where the
number and the entry cannot disagree. It carries the date, the sections `Keep a Changelog` names,
and a link to the release, and that link is dead until the draft is published. Keep it to what a
reader deciding whether to upgrade needs; the install steps and the checksums belong to the notes.

### 2. Prove the tree

```sh
task check          # toolchain pin, fmt, clippy -D warnings, tests
```

Run it on **both** platforms before tagging, and know that the numbers differ: two tests live behind
`#[cfg(all(test, target_os = "macos"))]` in `crates/km-video-downloader/src/alert.rs` and run
nowhere else. CI has no macOS runner, so a `task check` on a Mac is the only thing that compiles the
`#[cfg(target_os = "macos")]` code at all.

### 3. Merge, then tag

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

### 4. Watch the tag build

```sh
gh run watch $(gh run list --workflow release --limit 1 --json databaseId -q '.[0].databaseId')
```

It stages both programs, compiles the installer, installs and uninstalls it, checks the tag against
what the executable answers, and leaves a draft release with
`km-video-tools-setup-X.Y.Z-x86_64.exe` attached. **The run summary is where the notes come from**:
it carries that file's SHA-256, its size and the commit it was built from, which is what
**Verification** below asks for.

### 5. Build the macOS package, and attach it

From a Mac, on the tagged commit:

```sh
git pull --ff-only origin master
git rev-parse HEAD vX.Y.Z^{commit}    # the same sha twice, and a clean tree
task check
task clean:old                        # take away older staged versions -- see the trap below
task dist:setup:notarized
shasum -a 256 dist/setup/macos/<the artifact>
gh release upload vX.Y.Z dist/setup/macos/<the artifact>
```

Before attaching, confirm the number in every place it landed: the staged folder's name, both
binaries' `--version`, both staged `README.txt` files, and the package's own name. They are all
derived from one build, so they agree or something is stale.

> **The trap `clean:old` exists for.** A staged folder carries its version in its name, so a build of
> one version lands *beside* another rather than replacing it. A carrier that searched for its
> payload would find the wrong one, and everything downstream would then agree about it, because the
> version is read out of those same executables. `dist_staged_dir()` names the folder instead of
> searching, and `clean:old` stops there being a second one to find. Run it first.

### 6. Write the notes, and publish the draft

```sh
gh release edit vX.Y.Z   --title "vX.Y.Z — <the same phrase>"   --notes-file <notes describing both platforms>   --draft=false
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

**Short.** An opening line on what the release is for; **What is new** as bullets, one per thing
somebody would notice; **Install** with a paragraph per platform and the line that yt-dlp and ffmpeg
are not bundled; **Checksums**, the SHA-256 of each attached installer and nothing around it. See
`Release notes are what a reader needs to decide and to install` in `docs/decisions.md`.

What a build proved stays in the build. The round trips, the tag-against-binary check and the test
counts are in the workflow run and in `Checking a published release` below.

Write the notes for somebody who has not read the commits.

## Checking a published release

```sh
gh release download vX.Y.Z -p '<the artifact>' -D /tmp/check
shasum -a 256 /tmp/check/<the artifact>            # matches the notes
spctl -a -vv -t install /tmp/check/<the .pkg>      # accepted, source=Notarized Developer ID
xcrun stapler validate /tmp/check/<the .pkg>       # the ticket travelled with the file
```

Check the copy GitHub serves rather than the one on the build machine. That is the file people get.
