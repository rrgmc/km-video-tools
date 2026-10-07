# km-video-tools

Tools for getting karaoke **video** songs onto disk, in the shape a karaoke package wants them.

| | |
|---|---|
| **`km-video-fetch`** | The command line. It downloads a video with `yt-dlp`, writes the title and artist into its tags, and reports whether the file is playable. `--convert` does the same to a video already on disk. |
| **`km-video-downloader`** | The same fetch in a window. Set a folder, paste the links, press Fetch. A second page converts videos already on disk. |

## What it is for

A karaoke app that plays video songs wants one shape: **H.264 in 8-bit 4:2:0, at most 1080p30, AAC
audio, in MP4**. These tools ask the site for that shape, so the file that arrives needs no
re-encoding.

```sh
km-video-fetch '<url>' --out ./songs
km-video-fetch '<playlist url>' --playlist --out ./songs
km-video-fetch --from-file urls.txt --out ./songs

km-video-downloader                    # the same thing, with a window
```

`km-video-fetch --help` lists every option.

## Smaller files

`--video` asks for a smaller picture. The sound is the same at every size.

```sh
km-video-fetch '<url>' --out ./songs --video small   # 720p, roughly half the size
km-video-fetch '<url>' --out ./songs --video tiny    # 480p
```

The default is `full`, which is 1080p.

Some sites offer nothing as small as the size you asked for. `--normalize` re-encodes those videos
down to it, which takes minutes per song.

## Lists of links

An output folder records what was fetched into it, in `.km-fetched.txt`. Running the same playlist
again fetches only what is new.

A list is a text file with one link per line. A line may start with options for that link:

```text
# these follow --playlist, or its absence
https://youtu.be/aaaaaaaaaaa

--playlist    https://www.youtube.com/playlist?list=PLxxxx
--no-playlist https://youtu.be/bbbbbbbbbbb?list=PLyyyy

--out anime            https://youtu.be/ccccccccccc
--playlist --out jpop  https://www.youtube.com/playlist?list=PLzzzz
```

`--out` in a list names a folder under the output folder. Each folder keeps its own
`.km-fetched.txt`.

Lines above the first link are a header. They set options for the whole list:

```text
--cookies-from-browser firefox
--normalize
--limit 50
--video small

https://youtu.be/aaaaaaaaaaa
```

A header may carry `--playlist`, `--subs`, `--normalize`, `--no-archive`, `--limit N`,
`--cookies-from-browser BROWSER`, `--format SELECTOR`, `--sort ORDER` and `--video SIZE`. An option
on the command line overrides the same option in the header.

A list named `km-video-fetch.kmvf` in the output folder is that folder's own list. It is read when
you give no links and no `--from-file`, and the window offers it on the Fetch page.

The installers open `.kmvf` files with the window. Opening one fills the page in and fetches
nothing until you press Fetch:

```sh
km-video-downloader anime.kmvf
```

[`docs/design.md`](docs/design.md) has the full rules for a list.

## Videos you already have

`--convert` takes a video on disk and puts it in the same shape a download arrives in. Give it a
file, or a folder to convert every video in it. Repeat it to name more than one.

```sh
km-video-fetch --convert './rips/Band - A Song.mkv' --out ./songs
km-video-fetch --convert ./rips --out ./songs --video small
km-video-fetch --convert ./rips --out ./songs --dry-run   # say what would happen, do nothing
```

- The original file is never changed.
- The result goes into `--out` under the same name, with a `.mp4` extension.
- A file of that name already in `--out` is skipped, so running the same conversion again converts
  only what is new.
- A video already in the right shape is copied. A picture larger than `--video` is re-encoded down
  to it.

## The window

`km-video-downloader` opens its own window on Windows and macOS. `--browser` uses a browser tab
instead.

It has two pages. **Fetch** downloads links. **Convert** does the same for videos already on disk.
One job runs at a time, and Stop ends it. The output folder and the options are remembered between
runs.

The program serves its page on `127.0.0.1:8181`, which only this computer can reach. It has no
password and it writes files as you. `--lan` opens it to your network, so use it only on a network
you trust.

Windows has two executables. Double-click `km-video-downloader.exe`. Run
`km-video-downloader-console.exe` from a shell when you want `--help` or other printed output.

## What you need

Two other programs, neither bundled:

- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)** does the downloading. Keep it current, because
  sites change what they serve and an old copy fails in ways that look like a broken network. Pass
  `--yt-dlp PATH` when it is not on `PATH`.
- **ffmpeg**, with the `ffprobe` that comes in the same package, muxes, checks and re-encodes the
  files.

## Building

`rust-toolchain.toml` pins the compiler version, and the first `cargo` command in the checkout
installs it.

```sh
cargo build --workspace
cargo run -p km-video-fetch -- --help
```

With [Task](https://taskfile.dev) installed, `task --list` prints every task. The main ones:

```sh
task check        # format check, clippy and tests
task run -- '<url>' --out ./songs
task ui           # run the window
task dist         # stage a folder for each program into dist/
task dist:bin     # stage one folder holding every program
task dist:setup   # build the installer for this platform
task clean:old    # remove staged folders from other versions
```

`task dist` writes `dist/<app>/<platform>/<app>-<version>-<triple>/`. `dist/` is output and is
never committed.

`task dist:setup` builds one installer holding both programs. On Windows it needs
[Inno Setup](https://jrsoftware.org/isinfo.php) 6 (`winget install JRSoftware.InnoSetup`). On macOS
it needs nothing extra.

[RELEASE.md](RELEASE.md) covers signing, notarizing and cutting a release.

## Layout

```
crates/
  km-video-core/        the library: the yt-dlp arguments, running it, and checking the result
  km-video-fetch/       the command line
  km-video-downloader/  the window and its pages
icon/                   generated: `cargo run -p km-video-downloader --example icon`
tools/dist/             the scripts that stage dist/
tools/platform/         the Windows and macOS installers
docs/                   why things are the way they are
```

## Licence

MIT OR Apache-2.0, at your option.

## Author

Rangel Reale (realerangel@gmail.com)
