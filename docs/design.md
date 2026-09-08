# How `km-video-fetch` works, and why it works that way

## What it owns is the argv, and only the argv

It shells out and reads back what happened; it decodes nothing. Argument-building is a module of its
own (`km-video-core/src/args.rs`) for one reason: **the whole value of this tool is *which* arguments
it passes, and that is only assertable in a test if choosing them is separable from running them.**

## The three arguments that matter

A format cap at 1080, then a **sort** ranking what is left by codec — a sort rather than a longer
fallback chain: **a chain that runs out of alternatives fails the download, whereas a sort takes the
nearest thing and lets the check afterwards say what was settled for.**

And a conditional output template, so a video with a known artist is `Artist - Title.mp4` and one
without is `Title.mp4` rather than `NA - Title.mp4`.

**Whether a URL means one video or a playlist is stated rather than guessed.** yt-dlp's own default
takes the whole list from a URL carrying `&list=`, so somebody who pasted a link from a playlist page
gets two hundred songs they did not ask for. Here `--playlist` is a request.

**And a line of a list may state it for itself**, along with where it goes:

```text
https://youtu.be/aaaaaaaaaaa
--playlist https://www.youtube.com/playlist?list=PLxxxx
--out anime https://youtu.be/bbbbbbbbbbb
--playlist --out anime/openings https://www.youtube.com/playlist?list=PLzzzz
```

A line that says nothing takes the run's own answer, so a list with no markers in it means to this
tool what it means to yt-dlp. Nothing sniffs a URL's shape to decide: a link that looks precisely
like a playlist has still not said it is one.

Two consequences before writing such a list. **`--limit` is per playlist, not a budget for the
run**: five playlists at `--limit 10` is up to fifty videos. And it selects
by *index*, before the archive filters, so a video already fetched by a line of its own still
occupies a slot in the `1:N` of a playlist that contains it.

**And the list may state what is true of all of it**, in a header above the links:

```text
--cookies-from-browser firefox
--normalize
--limit 50

https://youtu.be/aaaaaaaaaaa
--out anime https://youtu.be/bbbbbbbbbbb
```

The two are different in kind. A *marker* says what one line is, and costs a yt-dlp run per
distinct answer, because `--yes-playlist` and `-P` are properties of an invocation. A *header
setting* is one field of the plan every run shares, so it splits nothing — and so the header can
carry `--cookies-from-browser` while a line may not.

The header is every line before the first link, blanks and comments included, and it ends at the
first line that is not a setting. That one rule is what keeps a bare `--playlist` unambiguous: at
the top it is the run's answer for lines that do not say, and in front of a URL it is that line's.
A line the header does not understand ends it and becomes a link, so a typo arrives as yt-dlp
saying it is not a URL, in the words of the program that would know.

**What is asked for wins, where it can be told to have been asked for.** `--limit`, `--format`,
`--sort` and `--cookies-from-browser` take a value, so *not given* is a thing a command line can
say and the list is heard. A flag cannot: off and unset are the same `bool`, on a command line as
much as in a form, so those are the or of the two and a list that says `--subs` cannot be talked
out of it. In the window the same settings arrive as **ticked boxes** rather than as behaviour, so
the page shows what the list asked for and lets it be changed.

A header may not say `--out` (the destination is where the file lives, and a list that moved its
own folder could not be copied anywhere), nor `--dry-run`, `--strict` or `--show-command` (which
are properties of an invocation — a folder that always simulates never downloads), nor `--yt-dlp`
(a fact about a machine, so a copied list would carry a path that is not there).

## Two things deliberately never passed

Both because they change the file's *stream layout* rather than its content:

- **`--embed-thumbnail`.** In MP4 this attaches cover art as a **second video stream**, and a reader
  that takes the first video stream it finds then describes the JPEG instead of the picture — one
  frame, no frame rate, and whatever pixel format the cover happened to be in, which would be
  reported as a file the machine cannot play. `probe.rs` skips a stream marked `attached_pic`, so
  there are two guards rather than one; a file fetched by other means can still arrive carrying cover
  art.
- **`--embed-subs`.** It muxes a `mov_text` stream, and the karaoke app does not index a video's
  captions. Available behind `--subs` for anyone who wants them.

**Restricting filenames to ASCII is refused for a different reason**: this material is Japanese and
Korean, the stem is the title of last resort, and mangling it is worse than a long one.
`--windows-filenames` is forced on **every** platform instead, so a corpus fetched on one machine and
one fetched on another are named identically.

## The record file's name goes through the output-template machinery

Neither of these is guessable from the documentation. **The record file's name is put through
yt-dlp's output-template machinery rather than taken as a path.** Consequently:

- **A filename-length limit shortens a long *absolute* path by dropping directory components**, which
  writes the record file one folder above the videos and leaves the tool reporting it fetched nothing
  at all, because that is where it looks. **A run that downloaded everything correctly says
  `nothing to fetch`.**
- **Sanitisation strips a leading dot**, so a hidden record file is written unhidden.

So the record file is named **relatively and without a dot**, and resolved against the output
directory. The download archive is *not* affected — that argument is an ordinary path — which is why
it keeps its dot and stays hidden. **Two tests assert exactly that asymmetry.**

## Progress is not piped, except where it must be

yt-dlp keeps the terminal and draws its own; only the machine-readable half goes to the record file.
That removes outright the failure a piped child would bring — **a child filling a pipe nobody is
draining** — which is the failure the ffmpeg call in `profile.rs` has to spawn a thread to avoid.

**A web page has no terminal to hand over**, so `run::spawn_watched` pipes after all and has to
avoid that failure the same way: stderr is drained by a thread of its own and stdout read on the
caller's. It also asks yt-dlp for a second, parseable progress stream:

```
--newline  --progress-delta 0.5
--progress-template "download:KMP status=%(progress.status)s pct=%(progress._percent_str)s
                     speed=%(progress._speed_str)s eta=%(progress._eta_str)s title=%(info.title)s"
```

which produces, verbatim:

```
KMP status=downloading pct=  2.4% speed= Unknown B/s eta=Unknown title=in-profile
KMP status=downloading pct= 49.4% speed=  20.49MiB/s eta=00:12 title=in-profile
KMP status=finished    pct=100.0% speed=16.57MiB/s   eta=NA      title=in-profile
```

**Four things the parser must survive, every one of them in those three lines:**

- **The values are space-padded to a fixed width.** `pct=  2.4%` split on its first space is an empty
  string, which parses as nothing and leaves a bar that never moves.
- **`Unknown` and `NA` are ordinary values**, not faults: speed is unknown for the first second of
  every download, and eta is `NA` on the line that says a file is done. Both become `None`.
- **`status=finished` ends one file, not the run.** A playlist emits it once per video.
- **The title is last in the template precisely so it may contain anything**, spaces and `=`
  included, so the fields are cut from the front by name rather than split apart.

Anything without the `KMP` prefix is yt-dlp talking to a person, and is passed through as such — that
is what fills the log the page keeps beside the bar.

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

`ffprobe` adds a wrinkle `ffmpeg-next` does not have: it reports **the container's own tag case**,
where the library normalizes MP4's `©nam` and Matroska's `TITLE` alike into lowercase `title`. Tags
are therefore looked up case-insensitively here.

**One consequence on the packaging side**: an incremental scan settles a file by its
size and modification time, so a corpus scanned before the tags existed keeps its blank artists until
those rows are scanned again. There is nothing to fix — **but it reads as a bug when a folder is
rescanned and nothing changes.**
