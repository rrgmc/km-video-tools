# km-video-tools

Tools for getting karaoke **video** songs onto disk, in the shape a karaoke package wants them.

Two programs over one library:

| | |
|---|---|
| **`km-video-fetch`** | The command line. Download a video with `yt-dlp` as H.264/AAC in MP4, write the title and artist into its tags, and say whether what arrived is playable. |
| **`km-video-downloader`** | The same fetch, as an application with a window. Set a folder once, paste the links or pick a file of them, press Fetch, watch it happen. |

They are the same program twice: both call `km_video_core::fetch::fetch`, and differ only in whether
its events become lines or a progress bar.

## What it is for

A karaoke machine that plays video songs wants one shape and not seventeen: **H.264 in 8-bit 4:2:0,
at most 1080p30, AAC audio, in MP4**. A download that took whatever a site offered arrives as VP9 at
60 fps and costs an hour of re-encoding per song; a download that *asked* for AVC and AAC arrives in
profile and gets copied.

That selector is not something anybody should be retyping, and neither are the two
`--parse-metadata` rules that put a song's title and artist into the container's tags at the one
moment yt-dlp still knows them. This is that command line, written down, plus a check on what
landed.

```sh
km-video-fetch '<url>' --out ./songs
km-video-fetch '<playlist url>' --playlist --out ./songs
km-video-fetch --from-file urls.txt --out ./songs

km-video-downloader                    # the same thing, with a window
```

A folder remembers what has already been fetched into it (`.km-fetched.txt`), and can carry its own
list of what to fetch (`km-video-fetch.txt`), so re-running over a playlist picks up only what is
new. The page reads that list too, and offers it as one click when it is there.

## The window

It is a real application on Windows and macOS: its own window, its own icon, no console. Inside the
window is a webview over the page the same process is serving, so one set of templates answers for
the window and for a browser tab alike — `--browser` asks for the tab, and a `--no-default-features`
build only has the tab.

On Windows there are two executables. `km-video-downloader.exe` is the one to double-click;
`km-video-downloader-console.exe` is the same program from a shell, where `--help` and the startup
address have somewhere to go. A program cannot choose at run time which it is — the subsystem is a
field in the PE header fixed by the linker.

It listens on `127.0.0.1:8181` — **this computer only**, because there is no
password on it and it writes files as you. `--lan` opens it to the rest of your network, for a
network you trust and only while you need it. It remembers the output folder and the options
between runs, in this platform's own config directory.

It is one page, in four parts: where the videos go, what to fetch, how, and what happened. A folder
is chosen by typing a path or by browsing — the listing is done on the server, because a browser will
not tell a page where a picked file lives, and a native dialog would mean a GUI toolkit on every
platform for one interaction. Progress is polled once a second, and yt-dlp's own output is kept
beside the bar, because in a terminal that is what somebody reads when a download fails and there is
no terminal here.

## What you need

Two other programs, neither bundled and neither linked:

- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)** — does the downloading. `--yt-dlp PATH` when it is
  not on `PATH`, which is common when `pipx` or a virtualenv installed it. **Keep it current**: sites
  change what they serve, and a copy a few months old fails in ways that look like a broken network.
  This tool prints the version it found and says so when it is stale.
- **ffmpeg** — yt-dlp muxes with it, `ffprobe` from the same package reads files back, and
  `--normalize` re-encodes with it.

Nothing here links either of them, which is why building this needs only a Rust toolchain — no C
compiler, no bindgen, no libclang.

## Building

```sh
cargo build --workspace
cargo run -p km-video-fetch -- --help
```

With [Task](https://taskfile.dev) installed, `task --list` prints what is routine. The four worth
knowing:

```sh
task check      # fmt, clippy, tests — in the order a failure is cheapest to read
task dist       # stage a folder somebody can be handed, into dist/
task dist:setup # ...or a setup program, for the people who would rather not unpack one
task run -- '<url>' --out ./songs
task ui         # run the window
```

`task dist` writes `dist/<app>/<platform>/<app>-<version>-<triple>/` holding the executable, both
licence texts and a README naming the version — plus the console twin on Windows, and a `.app`
bundle on macOS. `dist/` is output and is never committed.

`task dist:setup` builds the other kind of carrier: one installer holding both programs, with a
checkbox for each. It is deliberately not part of `task dist`, which would otherwise stage
everything twice.

On **Windows** it writes `dist/setup/windows/km-video-tools-setup-<version>-x86_64.exe`, an
[Inno Setup](https://jrsoftware.org/isinfo.php) installer that puts both programs in
`%LOCALAPPDATA%\Programs` **for your account only** — so it raises no UAC prompt — offers to add
itself to your `PATH`, and takes that entry back out when uninstalled. It needs Inno Setup 6
(`winget install JRSoftware.InnoSetup`), which it looks for in the per-user location `winget` uses
before the Program Files ones.

On **macOS** it writes `dist/setup/macos/km-video-tools-setup-<version>-<arch>.pkg`, an Apple
installer package: the application goes to `/Applications` and `km-video-fetch` to
`/usr/local/km-video-tools`, with a symlink in `/usr/local/bin`, which is already on your `PATH` —
so there is no “add me to your `PATH`” tick and nothing edits a `.zshrc`. It needs nothing that is
not already in the base system, and asks for your administrator password once.

Both **round-trip themselves on every build** — install into a scratch location, run each program,
uninstall, and assert nothing was left behind. Both are **unsigned**, so a recipient meets
SmartScreen or Gatekeeper on a first run; the fix for that is a purchased certificate rather than a
build step. Neither installer touches your downloaded videos or your settings when removed, and each
says so on its way out.

## Layout

```
crates/
  km-video-core/        the library: the yt-dlp argv, running it, ffprobe, the packaging profile,
                        and `fetch()` — the whole sequence, reported as events
  km-video-fetch/       the command line — argument parsing and what gets printed
  km-video-downloader/  the window and the page — tao, wry, axum, askama and vendored htmx,
                        every one of them compiled into a single file
icon/                   generated: `cargo run -p km-video-downloader --example icon`
tools/dist/             staging scripts; they write dist/, they never build into it
tools/platform/         the two setup programs: an Inno Setup .iss and a .pkg Distribution,
                        each with the script that drives it
docs/                   why things are the way they are
```

The rule the split exists to keep: **a binary crate here is a command line and its output.** Every
`println!` in `km-video-fetch` is in its `main.rs`, nothing in `km-video-core` prints at all, and
`km-video-downloader` renders the same events into HTML. That is what makes the two programs one
program with two faces rather than two programs that agree by accident.

## Where this came from

`km-video-fetch` was `tools/cmd/km-video-fetch` inside
[karaokemachine](https://github.com/rrgmc/karaokemachine) and moved out because it is the one thing
there that reaches the network for song material. That product downloads nothing, ever; the appliance
may have no internet at all. Keeping the downloader in a repository of its own makes that a fact
about the code rather than a paragraph in a document.

Two consequences are written down where they happen rather than only here:

- **`km-video-core/src/profile.rs` is a copy**, and karaokemachine's `km-pack` holds the
  authoritative one — it is what packaging actually enforces. See
  [`docs/decisions.md`](docs/decisions.md).
- **`km-video-core/src/probe.rs` reads a file with `ffprobe`**, where karaokemachine reads the same
  facts through `ffmpeg-next`. A second implementation, deliberately, so this stays a pure-Rust
  build.

## What it fetches, and from where, is your business

This runs yt-dlp against whatever it is pointed at. Downloading from a site may be contrary to that
site's terms, and the videos are somebody else's copyrighted work. Nothing here decides on its own
what to fetch: every URL came from a person.

## Licence

MIT OR Apache-2.0, at your option.
