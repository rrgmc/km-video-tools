# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Tools for getting karaoke **video** songs onto disk, in the shape a karaoke package wants them: two
programs over one library, and a `docs/` that says why each piece is the way it is.

## The documents, and which answers what

| Document | Answers |
|---|---|
| [`docs/decisions.md`](docs/decisions.md) | why things here are the way they are, as flat `##` entries |
| [`docs/design.md`](docs/design.md) | how `km-video-fetch` works — the argv it owns, the list grammar, the progress parser |
| [`README.md`](README.md) | what the two programs do, for somebody using them |
| [`RELEASE.md`](RELEASE.md) | cutting a release — the version's one home, and one setup program per platform |
| `crates/km-video-downloader/static/README.md`, `icon/README.md` | the two folders carrying a trap of their own |

**A decision is authoritative over anything here that disagrees with it**; where the two conflict,
the other one is out of date. A new product decision, or a changed requirement, gets a `##` entry in
`docs/decisions.md` rather than a paragraph in a commit message.

### Rules

1. **Prose states the rule and the reason, and never how the rule was arrived at.** No "used to", no
   "no longer", no version or milestone numbers, no sentence whose subject is the document, no
   appositive tail restating the clause before it. **This governs code comments and commit messages
   as much as documents**, and it is the rule most easily broken by somebody writing up a change
   they have just made. The full form, with the list of what creeps back, is
   `How a document in this repository is written` in `docs/decisions.md`, and `task lint:prose`
   catches the shapes that have one.
2. **What a user reads is written in plain application language.** The window's pages, the installer
   panes, `--help` and console output get labels and ordinary sentences; the reasoning behind a
   control belongs in the `{# #}` or `//` beside it. clap builds `--help` out of the `///` comments
   on the `Cli` structs, so a doc comment there is read by an operator and not only by a maintainer.
3. **A binary crate here is a command line and its output.** Every `println!` in `km-video-fetch` is
   in its `main.rs`, nothing in `km-video-core` prints at all, and `km-video-downloader` renders the
   same events into HTML. Anything two programs would both need lives in `km-video-core`.
4. **`Nothing committed describes the machine it was written on`** — no local drive or folder, no
   home LAN address, no personal hardware, no person, in prose or in a comment or as test data. A
   sample is invented; a reproduction step names a variable.

## Commands

With [Task](https://taskfile.dev) installed, and `task --list` for the rest:

```sh
task check        # toolchain-pin, fmt-check, clippy, tests — in the order a failure is cheapest to read
task lint:prose   # rule 1, over the lines this branch adds
task test         # cargo test --workspace
task run -- '<url>' --out ./songs
task ui           # the window
task dist         # stage a folder somebody can be handed, into dist/
```

**`task lint:prose` is deliberately not part of `task check`.** It reads the lines a branch adds
rather than the whole tree, which is what makes it runnable at all; the bare
`tools/dev/check-prose.sh` is the worklist for the rest.

**One test, or one module: `cargo test --workspace <substring>`.** Every test here is an inline
`#[cfg(test)]` module — there is no `tests/` directory in any crate, so there is no integration
target to name.

**A bash script is invoked through `"{{.SH}}"` and never a bare `bash`.** On Windows a bare `bash`
is WSL's, a different filesystem with no MSVC cargo behind it, and Task's embedded shell honours no
`#!` line. The `SH` variable in `Taskfile.yml` derives Git Bash from `git --exec-path`.

## Architecture

**Three crates, and the two programs are the same program twice.** Both call
`km_video_core::fetch::fetch` and differ only in whether its events become printed lines or a
progress bar.

`km-video-core` is the library, and `fetch` is where to start reading:

| Module | Holds |
|---|---|
| `args` | which arguments yt-dlp gets, and why each one |
| `list` | a list of links, and what a line of one may say about itself |
| `run` | finding yt-dlp, running it, and reading back what it did |
| `probe` | what a file that landed says about itself, read through `ffprobe` |
| `profile` | the shape a karaoke package wants, and the re-encode that reaches it |
| `size` | how much picture to ask for, and what a re-encode at that size aims at |
| `check` | three of those in the order that makes a verdict |
| `fetch` | all of it, end to end, reported as events |
| `child` | the one thing every subprocess has in common on Windows: no console window of its own |

**What this owns is the argv, and only the argv.** It shells out and reads back what happened; it
decodes nothing. Argument-building is a module of its own because the whole value of the tool is
*which* arguments it passes, and that is only assertable in a test if choosing them is separable
from running them.

**`km-video-downloader` serves the page it displays.** `tao` opens the window and owns the main
thread; `wry` puts the platform's own webview in it and points that at the loopback address the same
process is already serving, so one set of `askama` templates answers for the window and for a
browser tab alike. `--browser` asks for the tab, and a `--no-default-features` build has only
that.

**Two executables on Windows.** `km-video-downloader.exe` is the one to double-click and
`km-video-downloader-console.exe` the one where `--help` has somewhere to go; a program cannot
choose at run time which it is, the subsystem being a field in the PE header fixed by the linker.

**Neither yt-dlp nor ffmpeg is bundled or linked**, which is what keeps this a plain Rust build with
no C compiler, no bindgen and no libclang.

## Traps

- **The compiler version is not yours to choose.** `rust-toolchain.toml` holds the pin and
  `Cargo.toml`'s `rust-version` repeats it because no manifest format can derive it;
  `tools/dev/check-toolchain-pin.sh` fails if the two disagree, and `task check` runs it first.
- **`missing_docs = "warn"` is load-bearing.** Every item in this code carries a doc comment because
  of that lint, and the comments are where the reasoning is.
- **A heading in `docs/decisions.md` is quoted from outside `docs/`.** `.github/workflows/ci.yml`
  and `rust-toolchain.toml` each cite one by its full text and nothing validates the citation, so
  grep for a heading before rewriting it and change the citation in the same commit.
- **`dist/` is output and is never committed.** The staging scripts write it, never build into it.
- **The sibling project is "the karaoke app"** in every tracked file here, and its repository name
  appears in none of them.
