# km-video-tools

Tools for getting karaoke **video** songs onto disk, in the shape a karaoke package wants them.

One program today:

| | |
|---|---|
| **`km-video-fetch`** | Download a video with `yt-dlp` as H.264/AAC in MP4, write the title and artist into its tags, and say whether what arrived is playable. |

A local web UI over the same fetch is the reason this is a workspace rather than one crate; the
shared half already lives in `km-video-core`.

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
```

A folder remembers what has already been fetched into it (`.km-fetched.txt`), and can carry its own
list of what to fetch (`km-video-fetch.txt`), so re-running over a playlist picks up only what is
new.

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

With [Task](https://taskfile.dev) installed, `task --list` prints what is routine. The three worth
knowing:

```sh
task check      # fmt, clippy, tests — in the order a failure is cheapest to read
task dist       # stage a folder somebody can be handed, into dist/
task run -- '<url>' --out ./songs
```

`task dist` writes `dist/<app>/<platform>/<app>-<version>-<triple>/` holding the executable, both
licence texts and a README naming the version. `dist/` is output and is never committed.

## Layout

```
crates/
  km-video-core/     the library: the yt-dlp argv, running it, ffprobe, the packaging profile
  km-video-fetch/    the command line — argument parsing and what gets printed, and nothing else
tools/dist/          staging scripts; they write dist/, they never build into it
docs/                why things are the way they are
```

The rule the split exists to keep: **a binary crate here is a command line and its output.** Every
`println!` in `km-video-fetch` is in its `main.rs`, and nothing in `km-video-core` prints at all —
which is what will let a second program fetch and check exactly the same way.

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
