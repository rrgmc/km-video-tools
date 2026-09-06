# How `km-video-fetch` works, and why it works that way

Carried out of karaokemachine's `docs/architecture/video.md` when the tool moved. Everything here was
learned by running it.

## What it owns is the argv, and only the argv

It shells out and reads back what happened; it decodes nothing. Argument-building is a module of its
own (`km-video-core/src/args.rs`) for one reason: **the whole value of this tool is *which* arguments
it passes, and that is only assertable in a test if choosing them is separable from running them.**

## The three arguments that matter

A format cap at 1080, then a **sort** ranking what is left by codec — a sort rather than a longer
fallback chain, and that is a real choice: **a chain that runs out of alternatives fails the download,
whereas a sort takes the nearest thing and lets the check afterwards say what was settled for.** A
song that arrived as VP9 is still a song.

And a conditional output template, so a video with a known artist is `Artist - Title.mp4` and one
without is `Title.mp4` rather than `NA - Title.mp4`.

**Whether a URL means one video or a playlist is stated rather than guessed.** yt-dlp's own default
takes the whole list from a URL carrying `&list=`, so somebody who pasted a link from a playlist page
gets two hundred songs they did not ask for. Here `--playlist` is a request.

## Two things deliberately never passed

Both because they change the file's *stream layout* rather than its content:

- **`--embed-thumbnail`.** In MP4 this attaches cover art as a **second video stream**, and a reader
  that takes the first video stream it finds then describes the JPEG instead of the picture — one
  frame, no frame rate, and whatever pixel format the cover happened to be in, which would be
  reported as a file the machine cannot play. `probe.rs` skips a stream marked `attached_pic`, so
  there are two guards rather than one; a file fetched by other means can still arrive carrying cover
  art.
- **`--embed-subs`.** It muxes a `mov_text` stream, and karaokemachine decided not to index a video's
  captions. Available behind `--subs` for anyone who wants them.

**Restricting filenames to ASCII is refused for a different reason**: this material is Japanese and
Korean, the stem is the title of last resort, and mangling it is worse than a long one.
`--windows-filenames` is forced on **every** platform instead, so a corpus fetched on one machine and
one fetched on another are named identically.

## Two yt-dlp behaviors found the hard way

Both cost a debugging session, and neither is guessable from the documentation. **The record file's
name is put through yt-dlp's output-template machinery rather than taken as a path.** Consequently:

- **A filename-length limit shortens a long *absolute* path by dropping directory components.** The
  record file was written one folder above the videos, and the tool then reported it had fetched
  nothing at all — because that is where it looked. **A run that had downloaded everything correctly
  said `nothing to fetch`.**
- **Sanitisation strips a leading dot**, so a hidden record file is written unhidden.

The fix is one line and reads like a triviality without the reason attached: the record file is named
**relatively and without a dot**, and resolved against the output directory. The download archive is
*not* affected — that argument is an ordinary path — which is why it keeps its dot and stays hidden.
**Two tests assert exactly that asymmetry.**

## Progress is not piped

yt-dlp keeps the terminal and draws its own; only the machine-readable half goes to the record file.
That removes outright the failure a piped child would bring — **a child filling a pipe nobody is
draining** — which is the failure the ffmpeg call in `profile.rs` has to spawn a thread to avoid.

## Preflight

`--version` runs first, which doubles as proving yt-dlp can be run at all, and calls out a copy more
than 90 days old: **sites change what they serve and yt-dlp follows, and a stale copy fails in ways
that look like a broken network.**

Unlike ffmpeg, this accepts an explicit path, because yt-dlp is very often a `pipx` or virtualenv
install and genuinely often absent from `PATH` on a machine that has it. ffmpeg is checked too, **for
two reasons rather than one**: yt-dlp needs it to merge the separate streams *and* to write the tags —
without it, it leaves a `.webm` beside an `.m4a` and reports success.

## Container tags

The title and artist are read back from the container and **mapped to `None` when blank** — blank
rather than absent is the common case, because a muxer asked to embed metadata it does not have writes
the key with an empty value, and an empty string propagated onward is a song titled nothing at all.

`ffprobe` adds a wrinkle `ffmpeg-next` did not have: it reports **the container's own tag case**,
where the library normalizes MP4's `©nam` and Matroska's `TITLE` alike into lowercase `title`. Tags
are therefore looked up case-insensitively here.

**One consequence worth knowing on the packaging side**: an incremental scan settles a file by its
size and modification time, so a corpus scanned before the tags existed keeps its blank artists until
those rows are scanned again. There is nothing to fix — **but it reads as a bug when a folder is
rescanned and nothing changes.**
