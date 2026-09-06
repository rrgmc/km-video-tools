# Decisions

Why things here are the way they are. A decision is authoritative over anything else in this
repository that disagrees with it; where the two conflict, the other one is out of date.

## The repository exists so that karaokemachine has no downloader in it

`km-video-fetch` was `tools/cmd/km-video-fetch` inside
[karaokemachine](https://github.com/rrgmc/karaokemachine) and is the one thing there that reached the
network for song material.

**The line is between what a person asks for and what a machine decides to do on its own**, and this
tool has always been on the asked-for side: it runs yt-dlp against what somebody points it at and
fetches nothing on its own. That was true while it lived in the product's repository, and it was
still a paragraph in a document rather than a fact about the code. Now the product's repository
contains no downloader at all.

**Public rather than hidden.** The point of the split is the sentence above, not secrecy — a private
staging period while the extraction settles is a staging state and not the intent.

Downloading from a site may be contrary to its terms regardless of purpose, and the videos are
third-party copyrighted works. That is a matter for whoever runs the download rather than for this
repository, which is why the rule is **do not do it for anybody** rather than *do not be in that
business*.

## The packaging profile is copied, and karaokemachine's is authoritative

`crates/km-video-core/src/profile.rs` is a copy of `tools/cmd/km-pack/src/profile.rs` over there.

**That reverses the reason `km-pack` is a library**, which is that the profile should live in one
place, and the cost was accepted knowingly. Taking the real one meant taking `km-video` with it, and
`km-video` is `#![cfg(feature = "ffmpeg")]` at crate level over an unconditional dependency on the
machine's audio crate — so a downloader would link ffmpeg, need libclang at build time for bindgen,
and compile a synthesizer and an audio host, all to borrow one plain-data struct and one `matches!`.

**Where the two disagree, the other one is right.** It runs inside the packager, so what it accepts is
what a package actually contains; this copy only says, earlier, what that answer will be. Each file
carries a header saying so.

**Drift is bounded rather than merely hoped for.** The numbers are anchored to what the appliance's
decoder can draw — it copies three planes and contains no `swscale`, which is why the pixel format is
the only blocking finding — and they have not moved since they were written.

## A file is read with `ffprobe`, not with `ffmpeg-next`

A second implementation of what karaokemachine's `km_video::probe` does, deliberately.

**The subprocess costs nothing that is not already spent.** ffmpeg is a hard requirement of this tool
either way: yt-dlp muxes with it, and `--normalize` re-encodes with it. `ffprobe` ships in the same
package; a machine that can run one can run the other.

**What it buys is the whole build.** No linking, no C toolchain, no bindgen, no libclang — four
dependencies from crates.io and a `cargo build` that works anywhere. For a program whose entire job is
to run two other programs, linking a codec library to read ten integers out of a header was the wrong
trade.

Two things the library gave away that the subprocess has to do by hand, both covered by tests:
choosing the picture stream rather than the cover art, and looking tags up case-insensitively.

## The check is no longer optional

In karaokemachine the whole check-and-normalize half sat behind a default-off `video` feature,
because reaching the profile meant linking ffmpeg. A build without the feature downloaded exactly as
well and simply could not say whether what landed was playable.

With `ffprobe` there is nothing to gate, so **there are no features in this workspace at all** and
the tool's third reason to exist is always available.

## A binary crate here is a command line and its output

The repository is laid out as a workspace holding several programs — a local web UI over the same
fetch is the second one planned — so everything worth reusing lives in `km-video-core`, and a binary
crate holds argument parsing and printing.

Every `println!` in `km-video-fetch` is in its `main.rs`; nothing in `km-video-core` prints at all.
The boundary is *does it parse or produce a user interface*, which is why `km-video-core` does not
depend on `clap`.

`members = ["crates/*"]` follows from the same decision: a new program is a new directory and nothing
else.

## No `rust-toolchain.toml`

karaokemachine pins an exact channel, and that is right for an appliance whose build must be
reproducible and whose CI must not fail on a lint introduced on a Tuesday.

It is wrong here. This is a standalone tool other people compile with whatever their distribution
ships, and a pin turns "I have Rust installed" into "rustup will now download a second toolchain".
`rust-version` in the workspace manifest states the floor, and CI runs stable.

## `dist/` is output, and nothing else writes there

`tools/dist/cmd.sh` stages `dist/<app>/<platform>/<app>-<version>-<triple>/`, holding the executable,
both licence texts and a README naming the version. The folder carries the version and the triple
because it is a thing handed to somebody, and the first question they will have is which build they
were given.

**The version comes out of the built executable, not out of `Cargo.toml`**, so a staging run against a
stale build says so rather than mislabelling it. And **nothing spells `target/release/<x>`** — the
build directory moves, and a hard-coded path produces the worst failure this kind of script has:
cargo prints `Finished` and the next line says the executable was not produced. `dist_target_dir`
asks cargo.

## Nothing committed describes the machine it was written on

Inherited from karaokemachine and worth keeping: **no tracked file names a local drive or folder, a
home LAN address, personal hardware, or a person** — not in prose, not in a comment, not as test
data. A sample is invented; a reproduction step names a variable.
