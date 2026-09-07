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

A karaoke app that plays video songs wants one shape and not seventeen: **H.264 in 8-bit 4:2:0,
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
list of what to fetch (`km-video-fetch.kmvf`), so re-running over a playlist picks up only what is
new. The page reads that list too, and offers it as one click when it is there.

A line of such a list may say what it is and where it goes, which is the only way to fetch a mix of
single videos and whole playlists, or to sort what arrives into folders:

```text
# these follow --playlist, or its absence
https://youtu.be/aaaaaaaaaaa

--playlist    https://www.youtube.com/playlist?list=PLxxxx
--no-playlist https://youtu.be/bbbbbbbbbbb?list=PLyyyy

--out anime            https://youtu.be/ccccccccccc
--playlist --out jpop  https://www.youtube.com/playlist?list=PLzzzz
```

`--out` names a folder **under** the one the fetch was pointed at — a list may not reach outside it —
and each folder keeps its own `.km-fetched.txt`, because what is already in a folder is a fact about
that folder. A list with none of these markers in it is an ordinary yt-dlp batch file and is handed
over as one.

Lines **above the first link** are a header, and say what is true of the whole list:

```text
--cookies-from-browser firefox
--normalize
--limit 50

https://youtu.be/aaaaaaaaaaa
```

It may carry `--playlist`, `--subs`, `--normalize`, `--no-archive`, `--limit N`,
`--cookies-from-browser BROWSER`, `--format SELECTOR` and `--sort ORDER` — the settings a whole run
shares, spelled as the flags they override. What you pass on the command line wins over what the
file says; in the window they arrive as ticked boxes you can untick. A folder that always needs a
cookie jar can now say so once instead of being retyped every morning.

**`.kmvf` rather than `.txt`, and the extension is the point.** A list is a document with a grammar,
and `.txt` is the one thing that cannot say so to an operating system. With an extension of its own
it carries the program's icon and opens by double-clicking:

```sh
km-video-downloader anime.kmvf     # the window, that list filled in, nothing fetched
```

The output folder moves to the list's own folder and the list is offered ticked; pressing Fetch is
still yours. Opening a second one while the window is up hands it to that window rather than starting
a second copy — and so does opening the program itself again, which brings the window you already
have forward instead of failing over a port it cannot have. **Two different rules, worth keeping
apart:** a folder's own list is the one file named exactly `km-video-fetch.kmvf` sitting in it, while
the association is on the extension and opens any such file anywhere.

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

**The compiler version is not yours to choose.** `rust-toolchain.toml` names an exact one and pulls
`rustfmt` and `clippy` in with it, so the first `cargo` command in the checkout downloads what that
file asks for and `rustup update` is not part of building this.

```sh
cargo build --workspace
cargo run -p km-video-fetch -- --help
```

With [Task](https://taskfile.dev) installed, `task --list` prints what is routine. The ones worth
knowing:

```sh
task check      # fmt, clippy, tests — in the order a failure is cheapest to read
task dist       # stage a folder somebody can be handed, into dist/
task dist:bin   # ...or one folder with every program in it
task dist:setup # ...or a setup program, for the people who would rather not unpack one
task run -- '<url>' --out ./songs
task ui         # run the window
task clean:old  # take away the staged releases that are not this version
```

`task dist` writes `dist/<app>/<platform>/<app>-<version>-<triple>/` holding the executable, both
licence texts and a README naming the version — plus the console twin on Windows, and a `.app`
bundle on macOS. `dist/` is output and is never committed.

A staged folder is never overwritten by a build of a *different* version — the version is part of its
name — so yesterday's release sits beside today's until something takes it away. `task clean:old`
does, keeping the current version and removing the rest; `--dry-run` shows what it would take first.
`task clean` takes every staged release, and `task clean:all` adds what cargo built.

`task dist:bin` answers the other question people ask of a build — *give me one folder with all of
it in it*. It writes `dist/bin/<platform>/` and `dist/bin-console/<platform>/`: the first holds the
form you double-click where a program has one, the second the form that prints, and anything with
only one form is in both. `ZIP=1` also writes a versioned archive of each — the folders carry no
version, because a folder is where you keep the current build and the number belongs on the thing
you hand over. It gathers rather than builds, so nothing about what a staged folder holds is written
down twice.

`task dist:setup` builds the third kind of carrier: one installer holding both programs, with a
checkbox for each. It is deliberately not part of `task dist`, which would otherwise stage
everything twice.

On **Windows** it writes `dist/setup/windows/km-video-tools-setup-<version>-x86_64.exe`, an
[Inno Setup](https://jrsoftware.org/isinfo.php) installer that puts both programs in
`%LOCALAPPDATA%\Programs` **for your account only** — so it raises no UAC prompt — offers to add
itself to your `PATH` and to open `.kmvf` files with the downloader, and takes both back out when
uninstalled. It needs Inno Setup 6 (`winget install JRSoftware.InnoSetup`), which it looks for in the
per-user location `winget` uses before the Program Files ones.

On **macOS** it writes `dist/setup/macos/km-video-tools-setup-<version>-<arch>.pkg`, an Apple
installer package: the application goes to `/Applications` and `km-video-fetch` to
`/usr/local/km-video-tools`, with a symlink in `/usr/local/bin`, which is already on your `PATH` —
so there is no “add me to your `PATH`” tick and nothing edits a `.zshrc`. There is no tick for the
`.kmvf` association either: the application bundle declares the file type and LaunchServices notices
it, so removing the application removes the association with it. It needs nothing that is not already
in the base system, and asks for your administrator password once.

Both **round-trip themselves on every build** — install into a scratch location, run each program,
uninstall, and assert nothing was left behind. **Each also proves its own half of the association**,
because neither platform fails a build that quietly associates nothing. The Windows one installs it
and reads the keys back with `reg.exe`, which is the only way to catch a `[Registry]` entry Inno
skipped; the macOS one reads the document types out of the packaged bundle's `Info.plist` — that the
type it declares is the type it opens, that it names exactly one extension, and that the extension is
the one reported rather than a second copy typed into the check.

Both are **unsigned**, so a recipient meets SmartScreen or Gatekeeper on a first run; the fix for
that is a purchased certificate rather than a build step. Neither installer touches your downloaded
videos or your settings when removed, and each says so on its way out.

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

## Licence

MIT OR Apache-2.0, at your option.
