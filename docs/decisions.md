# Decisions

Why things here are the way they are. A decision is authoritative over anything else in this
repository that disagrees with it; where the two conflict, the other one is out of date.

## The repository exists so that the karaoke app has no downloader in it

**The karaoke app reaches the network for no song material at all.** `km-video-fetch` is the one
thing there that did, and nothing that fetches is added back.

**The line is between what a person asks for and what a machine decides to do on its own**, and this
tool is on the asked-for side: it runs yt-dlp against what somebody points it at and fetches nothing
on its own.

**The separation is structural rather than stated.** A document claiming the product does not
download can be out of date; a repository with no downloader in it cannot.

**Public rather than hidden.** A private staging period while the extraction settles is a staging
state and not the intent.

Downloading from a site may be contrary to its terms regardless of purpose, and the videos are
third-party copyrighted works. That is a matter for whoever runs the download rather than for this
repository, so the rule is **do not do it for anybody** rather than *do not be in that business*.

## The packaging profile is copied, and the karaoke app's is authoritative

`crates/km-video-core/src/profile.rs` is a copy of `tools/cmd/km-pack/src/profile.rs` over there.

**The profile should live in one place, and this is a second copy of it**, at a cost accepted
knowingly. Taking the real one means taking `km-video` with it, and `km-video` is
`#![cfg(feature = "ffmpeg")]` at crate level over an unconditional dependency on the machine's audio
crate — so a downloader would link ffmpeg, need libclang at build time for bindgen, and compile a
synthesizer and an audio host, all to borrow one plain-data struct and one `matches!`.

**Where the two disagree, the other one is right.** It runs inside the packager, so what it accepts is
what a package actually contains; this copy only says, earlier, what that answer will be. Each file
carries a header saying so.

**Drift is bounded.** The numbers are anchored to what the appliance's decoder can draw — it copies
three planes and contains no `swscale`, which is why the pixel format is the only blocking finding.

## A named size shrinks the download, and the sound is never re-encoded to save room

For karaoke the song is the audio and the picture is a caption over a background, so the picture is
where disk can be saved and the sound is where it cannot. `--video full|small|tiny` is that choice,
in `km-video-core/src/size.rs`.

**Every size lands inside the packaging profile, so the karaoke app never has to agree to this.**
`max_width`, `max_height` and `max_frame_rate_milli` are ceilings, so a 720p H.264/AAC MP4 is as much
in profile as a 1080p one and the packager copies its bytes either way. `Profile`, `DEFAULT` and
`Profile::check` are untouched; a test holds all three sizes inside them.

**The saving is in the download, and the re-encode is the fallback.** yt-dlp chooses the video and
audio streams separately and muxes them, so a height cap changes the picture and nothing else: the
audio of a 720p download is the audio of a 1080p one, to the byte. That costs no CPU and is where
the whole benefit is. Re-encoding a picture that arrived too large costs minutes per song, and a
site that offers 1080p almost always offers 720p, so it is the exception.

**`--video` is a size and `--normalize` is permission to spend CPU.** A file that arrives larger than
was asked for is reported as `Larger` and left alone. The alternative makes `--video small` silently
more expensive than asking for nothing, in the case hardest to notice: one video in a playlist of
forty with no smaller rendition on offer.

**The re-encode is this repository's own, and the profile beside it is still the copy.** How to reach
the shape is a different question from what the shape is: a packager re-encodes what it was handed,
and this chooses how much picture to spend disk on before anything is handed over.

**AAC audio is carried over rather than rebuilt.** Where a file already has the codec packaging
wants, `-c:a copy` makes the sound in the finished file the sound that arrived; anything else is
encoded to AAC at 192k as before. This also leaves a multichannel source at its own channel count,
which costs the profile nothing: it says nothing about channels, because the appliance resamples
whatever it meets to interleaved stereo `f32`. Drift from the karaoke app's copy is bounded to the
numbers its decoder can draw, and the audio codec is the one thing the profile says about sound.

## A local file is converted into the output folder, and the original is left alone

`km-video-core/src/convert.rs` runs the second half of a fetch over a file nobody downloaded:
`--convert PATH` on the command line, the Convert page in the window.

**The source is read and never written.** `check::normalize` replaces the file it re-encoded, which
is right for something yt-dlp wrote a second ago and wrong for a file somebody owns. The result is a
new file in the output folder under the source's own name, and what was named is still where it was.
`profile::transcode` is the primitive that writes elsewhere, and the in-place `check::normalize`
stays as it is for downloads.

**A file already in the shape packaging wants is copied rather than re-encoded.** Re-encoding it
spends minutes and a generation of picture to produce a file the packager treats exactly as it would
have treated the one that went in. The output folder is the whole result of a run, so a file that
needs nothing still lands in it.

**A name already taken in the output folder is a refusal.** That folder is somebody's library. It is
also what stops a `.mp4` sitting in the output folder being ffmpeg's input and its output at once,
which is the one ordering that loses a file.

**A size asked for is a reason to re-encode here**, the opposite of the rule in
`A named size shrinks the download, and the sound is never re-encoded to save room`. There a size is
a request to a site and a file that arrives larger is left alone rather than charged an hour of CPU
nobody asked for. Here the re-encode *is* what was asked for.

**A named folder is not descended into.** What somebody points at is what they meant, and a tree walk
turns one wrong path into hours of encoding. Which files a folder gives up is
`convert::VIDEO_EXTENSIONS`, matched by name rather than by opening each one: a folder of songs also
holds the archive, a list of links and cover art, and finding out by running ffprobe over each is a
subprocess apiece. A file named on its own is taken whatever it is called.

**Stopping takes effect between files.** ffmpeg is handed a whole file and offers no way to be asked
for half of one, which is the limit `Stopping is asking` in `km-video-downloader/src/job.rs` already
describes for a fetch.

**An option rather than a program of its own.** It shares the profile, the sizes, the re-encode and
the window with the fetch, and differs only in where the file came from. `--convert` is refused
beside the arguments that belong to a download, by clap rather than by hand, so an argv that cannot
mean anything is turned down before the program starts.

## A file is read with `ffprobe`, not with `ffmpeg-next`

A second implementation of what the karaoke app's `km_video::probe` does, deliberately.

**The subprocess costs nothing that is not already spent.** ffmpeg is a hard requirement of this tool
either way: yt-dlp muxes with it, and `--normalize` re-encodes with it. `ffprobe` ships in the same
package; a machine that can run one can run the other.

**What it buys is the whole build.** No linking, no C toolchain, no bindgen, no libclang — four
dependencies from crates.io and a `cargo build` that works anywhere. For a program whose entire job is
to run two other programs, linking a codec library to read ten integers out of a header is the wrong
trade.

Two things the library gave away that the subprocess has to do by hand, both covered by tests:
choosing the picture stream rather than the cover art, and looking tags up case-insensitively.

## The check is not behind a feature

**With `ffprobe` there is nothing to gate**, so the check is never optional and the tool's third
reason to exist is always available.

Reaching the profile through `ffmpeg-next` would mean linking ffmpeg, which is what puts a
check-and-normalize half behind a default-off `video` feature — and leaves a build that downloads
exactly as well and cannot say whether what landed is playable.

The workspace does have one feature — `km-video-downloader/desktop`, which is its window — and it is
on by default. That is a different kind of thing: it decides what a program *is*, not whether it can
answer a question it was asked.

## A binary crate here is a command line and its output

The repository is laid out as a workspace holding several programs, so everything worth reusing lives
in `km-video-core`, and a binary crate holds argument parsing and printing.

Every `println!` in `km-video-fetch` is in its `main.rs`; nothing in `km-video-core` prints at all.
The boundary is *does it parse or produce a user interface*, which is why `km-video-core` does not
depend on `clap` and `km-video-downloader` does not depend on `km-video-fetch`.

`members = ["crates/*"]` follows from the same decision: a new program is a new directory and nothing
else.

## A marked list is several yt-dlp runs, because yt-dlp has no per-URL playlist option

`--yes-playlist`/`--no-playlist` and `-P` are properties of an *invocation*. There is no per-URL form
of either to reach for, so a list that says one of its lines is a whole playlist and the next is one
video — or that two of them belong in different folders — cannot be one yt-dlp run. `fetch::runs`
sorts the list into one run per distinct `(expand, destination)` pair and `fetch::fetch` runs them in
turn.

**The split is in `fetch` and not in `args`,** which builds an argv, spawns nothing, and does no file
I/O at all. Deciding how many runs there are means reading the list. `runs` is a pure function of a
plan and a file for exactly the reason `argv` is a pure function of a plan: so what it decides can be
asserted by value rather than by running yt-dlp.

**A list that says nothing is handed over unread and unrewritten**, byte for byte the argv an
unmarked list builds. That keeps a `km-video-fetch.kmvf` somebody maintains by hand from being
rewritten behind their back, and keeps a list that carries things `list.rs` does not model working
at all. An unreadable list says nothing and is yt-dlp's to complain about.

**Single-video runs first, then playlist runs.** A folder's archive makes the first run win a
duplicate, and *this one video* is the more specific statement than *this playlist that happens to
contain it*; it is also the fast half, so somebody watching sees their named picks land before a
two-hundred-item playlist starts. Fixed rather than derived from the flag, because an order that
depends on a checkbox is worse to reason about and worse to test.

**Each destination keeps its own archive.** `ARCHIVE_NAME` already says an archive is a fact about
*this folder* — which songs are in it — carried with it if the folder is copied elsewhere. So a video
asked for in two folders lands in both, which is what asking for two folders means.

Four things about yt-dlp are load-bearing here, each read out of its `--help` or its behaviour
rather than assumed:

- **`--print-to-file` appends.** So the record file is removed *before* each run and read-and-deleted
  *after* it. Removing it only afterwards is correct by accident of ordering and not by rule.
- **`--batch-file` is an ordinary path argument**, resolved against the working directory and neither
  trimmed nor sanitised — `ARCHIVE_NAME`'s class, not `RECORDS_NAME`'s. So a generated list is passed
  with its folder on it, and the record file is passed as a bare name, and one test asserts both
  halves so the asymmetry cannot be half-remembered.
- **`#`, `;` and `]` all start a comment** in a batch file, and all three are skipped. Passing a `;`
  line through untouched costs nothing; *re-emitting* it as a URL is an extraction error for a line
  nobody meant to fetch, and a marked list is written back out.
- **A marked file is this tool's file, not yt-dlp's.** yt-dlp would read a marker as a comment or as
  part of the URL. Which is the strongest reason an unmarked list is never rewritten.

**`Event::Downloaded` is one per fetch**, carrying the combined haul. A second overwrites a caller's
total with the last run's count alone — and where that run fetched nothing, `job.rs` reads zero as
*nothing known yet* and leaves the bar sweeping for the whole checking phase of a run that succeeded.
The `Command` event does fire per run.

**The line sink captures the `Flow` that `run::spawn_watched` returns**, which is how `fetch` tells
that a watched run was stopped. `spawn_watched` folds a stop into success on purpose — a killed child
exits unsuccessfully, and reporting that as a failure would tell somebody who pressed Stop that
yt-dlp had broken. Across several runs nothing else carries the answer. With `|=`, not `=`: collected
stderr is replayed through the sink *after* the kill.

**A line may not name a folder outside the one the fetch was pointed at**, and the rule is one
sentence covering every shape: every component must be an ordinary name. That turns down `..`, a
leading separator, a drive letter and a UNC prefix without naming any of them. Refused before yt-dlp
starts, for the reason the cookie-browser check is: a fault that is knowable up front should not
surface as something that reads like the site said no. Refused rather than quietly clamped, because
`--out ../songs` in a list copied between two machines writes into whatever happens to sit beside the
destination on the second one.

**Three words and not "whatever yt-dlp takes".** A line that could carry a format selector or a
cookie browser is a much larger promise, and each one is another axis a run has to be split along.

## A list has an extension of its own, and something opens it

The list a folder carries for itself is **`km-video-fetch.kmvf`**, and the extension is registered
with `km-video-downloader` by both setup programs.

**`.txt` is the one thing that cannot be said about it.** A list is a document with a grammar —
`--playlist`, `--no-playlist` and `--out folder` in front of a URL, all of it in [`list`] — and an
operating system has no way to learn that from a name it shares with every other text file. A `.txt`
cannot be double-clicked into this program, carries no icon, and sits in a file manager as one more
text file.

**Two rules, and they are easy to conflate.** The list a folder carries for itself is matched by
exact filename, in the destination folder, only where nothing else was named. The association is
matched by extension, on any such file anywhere, and means only that somebody opened one. Neither
rule reaches the other: opening `anything.kmvf` does not make it a folder's own list, and a folder's
own list is found whether or not anything on the machine associates the extension.

**There is no fallback to `.txt`.** A folder carrying that name is not found, and is renamed by hand.
Two names would mean a precedence between them and a sentence in every explanation of the feature for
the life of the program.

**And nothing this program writes for itself carries the extension.** `ASKED_NAME`, `SINGLES_NAME`
and `PLAYLISTS_NAME` keep `.txt`, and a test pins it. Those sit in somebody's folder of songs for the
length of a run, and an interrupted run leaves them there for good; a program that wrote a
double-clickable file into a folder of songs would be offering to reopen its own workings.

## Opening a list fills the page in, and a second copy hands its own over

`km-video-downloader` takes one positional argument, which is all a shell association passes. It does
two things and no third: the output folder moves to that list's own folder, and the list is offered
on the page, ticked. **Nothing is fetched** — opening a document is somebody saying *look at this*,
not *do it*.

**The second double-click is what this removes.** A double-click starts a new process every time and
a second one cannot bind the port — on the GUI-subsystem executable, whose standard error goes
nowhere at all, it exits without a word and leaves no trace anywhere somebody would think to look.
That is the same class of failure as `println!` being a crash, below, and it is found the same way:
never from a shell, only from Explorer.

**The port is the whole mechanism.** No named pipe, no lock file, no single-instance mutex — the
copy that is already running is already an HTTP server on a known port, so the copy that cannot start
posts to it and stops. The instance with the window is the instance that answers.

**A list is the optional half of that message.** The rule is the plain one — **a copy that cannot
have the port hands over whatever it was opened with, including nothing at all** — and
`POST /opened` reads a body with no path as *come forward* rather than as a malformed request.

Handing over only where a file association passed a path answers the second double-click of a *list*
and leaves the second double-click of the *program* — somebody opening this while it is already open,
the commoner of the two — dying as described above. Such a condition reads as a guard and is a
narrowing. It is also unreachable on macOS, where a document arrives as an Apple Event and the
positional is always empty.

With nothing to take, the waking is the entire answer, which is what the test for it asserts; a 200
that woke nobody would be this program agreeing it had been opened and then staying behind whatever
is in front of it.

**And it says so where anybody can hear it.** A successful handoff goes through `say`, so the console
build tells somebody who typed the command twice why the second one exited without a window, and the
windowed build drops the line for want of anywhere to put it.

**`POST /opened` grants nothing new.** Anything that can reach that port can already post `/out` and
`/fetch` and make this program write files wherever it likes; that is what *it is on loopback, it
already writes files wherever it is pointed, and it runs as whoever started it* means. One more
endpoint on that surface is not one more capability.

**The request is written by hand over a `TcpStream`.** There is no HTTP client in this tree and this
is not a reason to add one: `reqwest` would bring a TLS stack and a second async runtime into a
program whose whole build story is a handful of crates and no C compiler, to talk to a socket on this
same machine.

**Every answer from that endpoint carries a marker, refusals included.** Identity and outcome are two
facts. With the marker only on success, *this program said no* and *that port belongs to something
else* are indistinguishable from the far end, and a path that is not there reads as a stranger on the
port.

**macOS reaches all of this by a different road and ends in the same place.** That platform delivers
a document to an application as an Apple Event rather than as an argument, so the positional is empty
there even on the launch that opened the file; `tao`'s `Event::Opened` is the arm that answers, and a
second file opened while the application is running is delivered to it by LaunchServices rather than
by a handoff. Windows never sends that event and macOS never sends the argument, so the two are not
alternatives to be chosen between.

**One case on macOS is improved rather than solved.** Where the copy holding the port is one
LaunchServices does not know about — the bare executable, or a `cargo run`, rather than the bundle —
a double-clicked list starts the bundle as a second process, and *that* process is the one the Apple
Event is addressed to. It exits at the failed bind, before an event loop exists to receive it, so it
hands over with no path: the window comes forward and the list is not shown.

Closing it means running an event loop in a process whose whole job is to exit — construct an
`NSApp`, wait a bounded moment for `Event::Opened`, hand over whatever arrived — for a case that
needs a copy running outside LaunchServices' knowledge, which is a thing developers have and users do
not. Not paid for.

**The opened list is drawn only where it is not already the folder's own** — which is the usual case,
since opening one moves the folder to where it sits. Two rows would be this program offering the same
file twice, under two names, with two counts to reconcile.

## A startup failure is put on the screen where there is no console to print it into

`main` returns a `Result`, and an `Err` out of it goes to standard error — the right arrangement
everywhere except the one place this program is actually started from. A GUI-subsystem executable has
a null standard error, and an application bundle launched by LaunchServices has one nobody will read.
So the program vanishes on startup and leaves nothing behind, which is the same silence `say` exists
for and the same one the handoff removes for a taken port.

**The handoff covers the common cause and this covers the rest**: a port held by a program that is
*not* this one, a directory that cannot be made, a runtime that will not start. Rare, and each is
otherwise indistinguishable from the program not existing.

**The check is on the terminal, not on the build.** `Shell::Console`, the `desktop` feature and
`windows_subsystem` are all proxies for *can anybody read what was printed*, and each is wrong
somewhere — the windowed build run from a terminal has a console, and a console build launched from a
file manager has none. `stderr().is_terminal()` asks the real question, so the dialog appears exactly
where the message would otherwise have gone nowhere and never as a second copy of something already
on screen.

**macOS only.** `osascript` is in the base system and this program
already shells out to `open` for a browser — one more child, no dependency, the shape `opener.rs` is
already in. Windows would want `MessageBoxW` and therefore `windows-sys`, the first such crate in a
tree whose whole build story is a handful of crates and no C compiler; and it already has the answer
this would duplicate, because `km-video-downloader-console.exe` exists precisely to be the build that
can talk.

**The whole chain is shown, not the outermost sentence.** The outer ones here are written for a
person and the inner one is what happened: *asking the copy already running to come forward* above
*something else is listening on 127.0.0.1:8181* is the pair that identifies the fault. Capped,
because an error is not a log and a dialog is not something anybody can scroll. Escaped, and that has
a test on it — an unbalanced quote does not mangle the dialog, it makes the script unparseable, so
the report of last resort becomes the thing that fails to appear.

## Each setup program proves its own half of the association

Neither platform fails a build that associates nothing, and the two fail silently in the same way for
different reasons: Inno reports nothing about a `[Registry]` entry a mistyped condition skipped, and
`documents()` writes the macOS keys through a heredoc nested inside another heredoc, where a slip
yields a package that installs cleanly and an application LaunchServices files under nothing at all.
In both cases the first person to find out is somebody double-clicking a list.

So the Windows driver reads its keys back with `reg.exe` and the macOS driver reads the document types
out of `Info.plist` with `plutil`. **Out of the Payload, not out of the staging directory**: what is proved has to be what somebody
receives, and a correct staged bundle beside a wrong archive is exactly the failure the round trip is
for.

Four things are asserted, and each is one that can be wrong on its own: the plist parses at all; the
type the bundle *declares* and the type its document entry *opens* are the same string, a mismatch
being the macOS shape of an extension registered to a ProgId with no command behind it; there is
exactly one filename extension; and `LSHandlerRank` is `Owner` beside a `CFBundleIdentifier` for
LaunchServices to file the declaration under. **The extension is read rather than repeated** — one
place decides it for this platform, and a copy typed into the check is free to drift from it. The
Windows driver reads its own out of the `.iss` for the same reason.

**Each assertion is watched to fail**, against a deliberately broken `documents()` — a mismatched
identifier, a dropped tag specification, an unbalanced tag — because a check that has never been seen
to refuse anything is not known to check anything.

## The bundle states the macOS floor, and two files agree about it

`11.0`, what the wry/tao stack needs, is stated in both places this is handed over by.
`distribution.xml` binds a `.pkg` install and nothing else; the other way is the staged folder, and a
bundle without `LSMinimumSystemVersion` dropped on an older Mac fails in the dynamic loader before
`main`, which is the same failure with none of the explanation.

So the bundle states it too, out of `common.sh`'s `dist_min_macos`. Neither copy can be derived from
the other — one is an XML attribute `productbuild` reads, the other a plist key LaunchServices reads —
so what is left is what `rust-toolchain.toml` and `Cargo.toml` already do here: state it twice and
refuse a build where the two disagree.

## The macOS installer's conclusion pane is where a GUI-only install reads

`dist_installed_readme macos` is written into the **fetch** component's payload, landing in
`/usr/local/km-video-tools` beside the program it describes. The consequence is that somebody who
unticks `km-video-fetch` installs no README anywhere and never reads a word of it.

**Not fixed by putting one beside the application.** A loose `README.txt` in `/Applications` is
against the platform, and inside the bundle it is a file nobody opens. The Installer's own conclusion
pane is what a GUI-only install actually reads, so that is where the association is explained — and
the welcome pane says it is coming, since Windows offers it as a visible tick and macOS offers no
moment at all.

## The whole fetch is a library function that narrates

`km_video_core::fetch::fetch(&Request, on_event)` runs the sequence — preflight, argv, spawn, read
back, check, re-encode — and calls back as it goes. `km-video-fetch` renders those events as lines;
`km-video-downloader` renders them as a progress bar and a list.

**A function that prints cannot be reused by a second front end.** Keeping the sequence in
`km-video-fetch`'s own `run()`, interleaved with the printlns that report it, is a perfectly good
shape until a web page wants the same sequence — and a web page cannot call a function that prints.

Two consequences keep the two faces honest:

- **The events are a narration, not a state machine.** They arrive in the order things happen and
  each is complete in itself. A caller that ignores every one of them still gets the `Outcome`.
- **`km-video-core` decides no wording.** Every sentence a person reads is in the crate that shows
  it, so the command line's output can move without changing a word of it.

The line sink also returns a `Flow`, which is how Stop reaches a running fetch. That is a second job
for a closure already being called at every point where stopping is possible — as against a flag the
library would have to be handed and remember to read.

## yt-dlp keeps the terminal where there is one, and is piped where there is not

`run::spawn` inherits stdout and stderr, so yt-dlp draws its own progress bar. It is a better bar
than anything reconstructed from a pipe, and there is no pipe to deadlock on.

`run::spawn_watched` pipes, because a page has no terminal to hand over. It answers the deadlock the
module header warns about the same way `profile::transcode` does — **stderr on a thread of its own,
stdout on the caller's** — which also keeps the line sink off any thread but the caller's, so it
needs no `Send` bound. The cost is that stderr arrives in a block at the end; with `--newline` in
effect nearly everything is on stdout, and what stderr carries is the errors, which is the part read
afterwards anyway.

Watched mode also asks yt-dlp for `--progress-template`, whose output is parseable where its bar is
not. Both directions are tested: the template in a terminal run would replace a good bar with a wall
of text, and a missing one in a watched run leaves a page with a bar that never moves.

## Progress is polled, not pushed

`hx-get="/progress" hx-trigger="every 1s"`, with **the polling attributes emitted only inside the
`running` branch** — so the last frame ends the loop by being the last frame. Nothing has to switch
polling off, and a page reloaded mid-fetch picks the job back up, because the fragment renders from
what the server knows rather than from anything the browser held.

Server-sent events would be the alternative and are not worth it here: they earn their keep pushing
sub-second state to several open pages at once, and this is one page watching one job whose fastest
meaningful change is about twice a second. The polling version is a template and no client code.

## The page is light, and says so

Its models in the karaoke app — `km-package-builder`, `km-admin`, the machine's own screen — are
dark, because they sit beside an appliance in a dark room. This is a tool used at a desk in the
daytime and it belongs to a different repository, so it leads with white.

**`color-scheme: light` is declared, and is not `light dark`.** A scheme left open lets the browser
draw form controls from the other one, which is how a light page ends up with dark dropdowns. There
is no `prefers-color-scheme` block: this program has one appearance and states it.

## It is an application with a window, not a program that prints an address

**A browser tab is not the whole user interface.** Double-clicking the executable to be shown a
console with an address in it is not an application.

The cost of a window looks like four of the karaoke app's crates, because that is what km-admin's
`desktop` feature wants: `km-tray`, `km-console`, `km-osopen` and `km-logfile`. Only two are real.
The tray is genuinely optional, `opener.rs` replaces one in fifteen lines, and the console shim is
replaced by [`say`] not being `println!` (below).

So `desktop` is a feature and it is **on by default**, because the point of a default is what
somebody gets without knowing there was a choice. `--no-default-features` still builds the
browser-only program, which is what a Linux machine without libwebkit2gtk needs: `wry` links it at
load time, so a build carrying the feature does not *start* there — a failure in the dynamic loader,
before `main`, that no flag can rescue.

The window is a **webview over the page this same process is serving**, which is the arrangement all
three of the karaoke app's do: one set of templates answers for the window and for a browser tab
alike, so there is never a second front end to keep in step. `--browser` asks for the tab.

## `println!` is a crash in a GUI-subsystem executable

`std::io::_print` **panics** on a write failure — `failed printing to stdout` — and a
GUI-subsystem executable on Windows has a null standard output handle that fails every write. Left
as `println!`, every double-click aborts the process, and it never once fails when run from a shell,
which is where it would be tested.

`say()` writes and drops the error. There is nowhere to report an error about there being nowhere to
report.

## Two executables, because the subsystem is a link-time field

`km-video-downloader.exe` is GUI-subsystem so no console appears beside the application;
`km-video-downloader-console.exe` is the same library, console-subsystem, for a shell that wants
`--help` and the address. A program cannot choose at run time — the subsystem is a field in the PE
header fixed by the linker — so it is two binaries or it is neither.

## A GUI subsystem is also a promise about children

The subsystem is not only a linker field. A GUI-subsystem process has **no console for a child to
inherit**, so Windows gives each child one of its own — and this program's whole job is running
other programs. Left alone it is a black window per yt-dlp run, one per file re-encoded, one per
`ffprobe`, and one more for the browser it opens at startup.

So every spawn carries `CREATE_NO_WINDOW`, through `km_video_core::child::without_a_console_window`
— **except one**. `run::spawn` is the command line's path and hands its inherited terminal to yt-dlp
so yt-dlp draws its own progress bar there; the flag detaches a child from the console it was given,
which is exactly what that call exists to pass along. It also buys nothing there, a console-subsystem
parent having a console already.

**Hide the console wherever the child's output is captured or discarded; never where the child is
handed a terminal on purpose.** Every other call already pipes or nulls its output, so the window
shows nobody anything.

**Changing the subsystem is what makes this load-bearing.** While a program is console-subsystem the
helper is merely tidy and a spawn site without it costs nothing; the moment the executable is
GUI-subsystem, every such site is a black window. A subsystem change reaches every spawn in the tree
or it reaches none of them.

## The icon is the same drawing under a fifth palette

Angular bands, a near-black plate, `KM` with a coloured M: the karaoke app's mark, because these
programs are run beside its and belong to it. **A vermilion lead**, chosen by hue distance rather
than by taste: its four sit at 45°, 148°, 196° and 324°, and this is 11° — 34° clear of the
nearest. A cyan at 180° is refused for the same reason: 16° from the package builder's blue, which
is exactly the confusion a per-program palette exists to prevent.

The other opening, around 260°, is a violet and is refused: the tile's own ground is a deep violet
and its middle band a magenta, so a violet lead would make the whole icon one hue with nothing to
catch at 16 pixels.

`crates/km-video-downloader/examples/icon.rs` draws it and writes `icon/`. Its geometry is a copy of
the karaoke app's renderer, which reads colours out of `km_display::theme::Theme` and types out of
SDL, neither reachable from here. **The copy may drift**: these are different programs' icons and are
supposed to differ.

The `.ico` goes inside the Windows executable through `build.rs`, which is the only way Explorer,
the Start menu and the taskbar have an icon *before* the process starts. The `.icns` is written
without `iconutil`, so a macOS bundle can be staged from any machine.

## An unknown cookie browser is refused before anything is downloaded

`--cookies-from-browser frefox` is **not** a usage error yt-dlp turns down at the door. It starts,
extracts, and fails on the first video with a message that reads like the site said no. So
`args::COOKIE_BROWSERS` writes the nine down and `args::browser_is_known` checks the part before
`+`, `:` and `::` — the browser, never the profile, which is a name on somebody's own machine that
nothing here could know.

Checked in two places on purpose: the page refuses it as a form problem, with its own words and
before a job exists, and `fetch()` refuses it as a command it knows cannot work. The second is the
backstop and is what the command line gets.

The field is an `input` with a `datalist` rather than a `select`, because the full syntax is
`BROWSER[+KEYRING][:PROFILE][::CONTAINER]` and somebody with two Firefox profiles has to be able to
say which. The list offers the nine; the field accepts the rest.

## The folder picker is a server-side listing

**A browser will not tell a page where a picked file lives** — a file input gives contents, not
locations — and there is no folder input at all. So the listing is done on this side: the server reads a directory, the page draws it, a click asks
for the next one.

That grants nothing new. The program is on loopback, it already writes files wherever it is pointed,
and it runs as whoever started it.

Deliberately **not** a native dialog, which would mean a GUI toolkit on every platform for one
interaction in a program whose interface is otherwise a page.

The picked *file* of links is the other half of the same fact, read the other way round: what the
browser sends is the file's bytes, and the bytes are all that is wanted.

## The window has two pages over one job

The bar chooses between Fetch and Convert. They are plain links and a whole page load, which is what
makes the address bar, the back button and a reload each mean what they look like.

**One job slot between them**, which is why Stop is `POST /stop` and belongs to neither page. A
second run is refused while one is going, exactly as two fetches always were: there is one progress
bar and one list of results, and a machine re-encoding video is busy.

**The shared fragments are told which half is drawing them**, through htmx's own `HX-Current-URL`.
Three things turn on it: setting the output folder redraws the page it was set on, the note under the
results explains the verdicts that half produces, and the log is labelled for the program whose words
are in it. A request without the header is the Fetch page, which is where an ordinary form post from
a page whose script did not load comes from.

**The folder picker lists files when the Convert page asks it to.** The reason a picker exists at all
is `The folder picker is a server-side listing`, and it applies twice over here: a browser hands a
page a picked file's contents rather than its location, and a video is gigabytes. So the path is what
travels, and a click adds it to the box rather than posting it — a conversion takes several paths,
and somebody is usually picking the second one.

## The Rust toolchain is pinned exactly

**`rust-toolchain.toml` names one `x.y.z` version, and it is the only place the number is decided.**

The argument against a pin is that this is a standalone tool other people compile with whatever
their distribution ships, so one turns "I have Rust installed" into "rustup will now download a
second toolchain". That is the right answer for a library and this is not one — two programs and the
crate they share, every member `publish = false`, built here and handed to somebody as a folder or an
installer. Nothing compiles against it, so the pin is imposed on nobody but whoever works on it.

**A floating channel does not cost reproducibility in the abstract.** `task lint` is clippy with
`-D warnings`, so a lint introduced upstream on a Tuesday fails a branch that changed nothing
relevant, and "passes locally" means only "passes on whatever this machine last fetched". It also
breaks outright: a `rust-version` of 1.98.1 inherited from the karaoke app's pin, against a `stable`
of 1.98.0, makes every cargo command in the workspace refuse before it compiles anything. A pin is
what makes those two numbers one decision instead of a race.

**The number propagates rather than being repeated.** `.github/workflows/ci.yml` installs with
`rustup toolchain install --no-self-update`, which resolves the file, `components` and all;
`dtolnay/rust-toolchain` is deliberately not used, its `toolchain` input being required and unable to
read the file, so keeping it would mean the version written twice. `Cargo.toml`'s `rust-version` is
the one copy that no format lets us derive, and `tools/dev/check-toolchain-pin.sh` — which
`task check` runs — fails naming a disagreement, a floating channel, or a returning `dtolnay` step.

The cost is that a bump is a commit rather than a `rustup update`. The arrangement is copied from
the karaoke app.

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

## A staged folder is chosen by the manifest and named by the binary

`dist_fresh_dir` clears only the folder it is about to write, and a folder's name carries its version
— so a build of 1.8.0 does not replace 1.7.0, it lands beside it. `task clean:old` is what takes the
older one away, and until it is run something has to decide which of the two is *the* staged build.

**Picking the first glob match is the wrong answer, and it fails silently.** A glob of
`<app>-*-<triple>` expands in sorted order, so with 1.7.0 and 1.8.0 both present it returns 1.7.0,
and nothing downstream disagrees: `bin.sh` gathers those executables and reads the version out of one
of *them*, and both setup programs then read it out of that payload in turn. The result is an
installer correctly labelled `1.7.0` carrying a build nobody asked for, on both platforms, with no
failure anywhere for anybody to notice — the same class of mistake as a cleaner that reports success
while matching nothing.

`dist_staged_dir` in `tools/dist/common.sh` names the folder instead of searching for one, out of
`dist_pkg_version` — one number, because every crate here is `version.workspace = true`.

**This is not a breach of "from a binary, never a manifest."** That rule answers *what is this
artifact*: the version printed, the folder named by `tools/dist/cmd.sh`, and the number on every
installer all come from a binary's own `--version`. The manifest answers a different question —
*which artifact did we mean* — and the two cannot contradict each other, because the folder's name
was built out of that binary's answer in the first place. A current build that is not staged says so
rather than being quietly replaced by an older one.

## `task clean` is a script, because Task's shell has no `rm`

Task runs every command through its own embedded POSIX shell, which has `for`, `case` and parameter
expansion everywhere but no `rm` — that is an external command, and there is no `rm.exe` on Windows
any more than there is a `sed`. So a `clean: rm -rf dist` written in `Taskfile.yml` is

    "rm": executable file not found in $PATH
    task: Failed to run task "clean": exit status 127

from a PowerShell or a `cmd`. It works only from a Git Bash — the one shell whose `PATH` lends Task
a coreutils it does not otherwise have — so such a task is broken in precisely the situation the `SH`
variable exists to serve and looks fine in the one it does not need to.

All three clean tasks are `tools/dist/clean.sh`. The Taskfile chooses which script to run, which is
what it does for staging, and the deleting happens where coreutils exist. Inherited from the karaoke
app, whose `clean:old` this is a port of.

**`--old` has no special cases.** An entry is removed only when its
name begins with its app's own name followed by a version that is not the wanted one, so anything
unrecognized survives without being named here: the versionless `dist/bin/<platform>`, the generated
directory the Windows installer clears itself, the macOS bundle whose number lives in its
`Info.plist`, and the contents of any folder being kept.

## The staging scripts carry the executable bit, and `common.sh` does not

`tools/dist/cmd.sh`, `tools/dist/bin.sh` and both `tools/platform/*/installer.sh` are mode `100755`
in the index. `tools/dist/common.sh` is `100644`, because it is sourced and never run — the mode is
the only place that distinction is written down where a reader will meet it before the header
comment.

**This is load-bearing, and it fails on exactly one platform.** The Taskfile invokes every one of
these through `{{.SH}}`, which is a bash that ignores the mode; Git Bash on Windows reports every
file as executable regardless. So neither the tasks nor a Windows checkout can tell the bit is
missing. But the scripts also invoke *each other* directly — `installer.sh` runs `cmd.sh`, `bin.sh`
runs `cmd.sh`, the Windows driver runs `bin.sh` — and on macOS and Linux that is

    tools/platform/macos/installer.sh: line 91: tools/dist/cmd.sh: Permission denied

after the tool check has already passed and printed `== macos installer`. Each script's own usage
header documents it as directly runnable too, which without the bit is untrue everywhere but
Windows.

## Nothing committed describes the machine it was written on

Inherited from the karaoke app: **no tracked file names a local drive or folder, a home LAN
address, personal hardware, or a person** — not in prose, not in a comment, not as test
data. A sample is invented; a reproduction step names a variable.

## How a document in this repository is written

**A document states the rule and the reason somebody would need in order not to undo it. It does not
narrate how the rule was arrived at.**

What that excludes, in order of how often it creeps back:

- **What something `used to be`.** No "former names", "former defaults", "former behaviors", no
  "this reverses", "this used to say", "since renamed", "no longer". A reader arrives at the
  repository as it is; a sentence about a state that is gone costs them a paragraph and tells them
  nothing they can act on. This covers a decision that was reversed as much as a spelling that
  changed.
- **Chronology.** No milestone numbers, no dates in headings, no ordering of when things were found.
  A date belongs in the body only where a reader needs to know when a measurement was taken.
- **Meta-commentary on the writing.** Any sentence whose subject is the document:
  "recorded rather than glossed", "and that is the record of it", "worth saying out loud".
- **Reassurance and common sense.** A paragraph explaining that a first run works, or that a
  diagnostic is optional, is a paragraph nobody needed.
- **Appositive tails.** "…, which is what makes X safe", "…, and that is deliberate rather than an
  accident" — the clause after the comma usually restates the clause before it.

What survives is the imperative and the trap. **A heading that instructs is not verbose** —
`Nothing committed describes the machine it was written on` *is* the content. A heading that merely
describes is trimmed to a plain noun phrase.

**The keep-test for a paragraph**: would a reader who deleted it either re-derive a wrong answer, or
break something silently? Anything in the past tense about a decision that was reversed fails it.
When unsure, keep the sentence and delete the paragraph around it.

**This applies to code comments too**, on the same test. A comment saying why a line is the way it
is earns its place; one describing a state that is gone does not. It applies to a commit message as
well, which is where a change is most naturally narrated.

**`tools/dev/check-prose.sh` keeps the mechanical half true**, and `task lint:prose` runs it over
the lines a branch adds. It matches the three shapes above that have one -- `what something used to
be`, `chronology`, `meta-commentary` -- and cannot see the appositive tail or the paragraph of
reassurance, so a clean run is a floor rather than a pass. It is deliberately **not** in
`task check`: `--changed` reads what a branch adds by line rather than by file, so touching a file
does not inherit that file's backlog, and the whole-tree form is the worklist for the rest.

**A phrase inside quotation marks or a code span is read as a mention rather than a use**, which is
what lets this entry name the shapes it forbids. So the bullet leads above carry them quoted or in
backticks, and a new one does the same — otherwise the check reports this entry as a violation of
itself. Two patterns are deliberately absent for the same reason the filter exists: a bare
`stated rather than`, because `stated rather than guessed` is how `docs/design.md` and
`km-video-core` say a URL is read, and `worth knowing`, because `Taskfile.yml` uses it of a flag. A
pattern that asked for those to be reworded would be the checker deciding the prose.

**A heading is quoted from outside `docs/`.** `.github/workflows/ci.yml` and `rust-toolchain.toml`
each cite one by its full text, and nothing validates the citation. Grep for a heading before
rewriting it, and change the citation in the same commit.

**The em-dash stays.** It is how a parenthetical mechanism is punctuated here. What goes is the
clause after it that restates the clause before it, not the dash.

## The changelog records what changed, and every other document states what is

`CHANGELOG.md` holds one entry per release, and is the only file here licensed to be chronological.
Everything else describes the repository as it is.

**A tree that only states its present cannot answer the one question an upgrade turns on**, which is
what moved between the version somebody has and the version they are looking at. The releases on
GitHub answer it and are not in a checkout, so a reader offline, or reading a diff, or deciding
whether a `git pull` is worth it, has nowhere to look.

**An entry says what changed and links to the release.** The install steps, the SHA-256 of every
artifact and what a build's round trip proved stay in the release notes, where `RELEASE.md` puts
them. That boundary is what keeps two descriptions of one release from drifting: the changelog holds
what is short enough to check at a glance, and nothing that has to be regenerated.

**It is still a second place a release is described**, which is a cost paid nowhere else here. It
buys a history that travels with the checkout, and the entry is kept short so that the two cannot
diverge far.

**The layout is Keep a Changelog, and it reads unlike anything else here.** Sections named `Added`,
`Changed` and `Fixed` over bullets are a convention a reader meets already knowing how to skim it,
and one a tool can parse. In the file somebody opens to compare two versions, that is worth more than
a voice shared with the documents around it.

**`tools/dev/check-prose.sh` skips it**, because `used to` and `no longer` are what an entry is made
of rather than a lapse into narration. It is the only path exempt for what it says; the other two are
exempt for what they are.

**The entry is written in the version-bump commit**, which is the one arrangement where the number
and the entry cannot disagree. `RELEASE.md` step 1 says so. The link it carries points at a release
that does not exist until step 5.

## What a user reads is written in plain application language

**Every surface a person uses speaks the plain, conventional English of a software application.**
Short labels, ordinary sentences, standard terminology. Three readers, and the register is theirs
rather than the writer's:

| Reader | Surfaces | Register |
|---|---|---|
| somebody fetching videos | the window's pages, its alerts, the installer panes | labels; at most one short sentence |
| an operator setting it up | `--help`, console output, the folder picker | a sentence or two, consequence first |
| a maintainer reading the source | code comments, `docs/` | the reasoning, at whatever length it takes |

**A page is not a comment.** Where the reasoning behind a control is worth keeping, it belongs in the
`{# #}` or `//` beside the markup. It does not belong on screen, where it costs a reader who came to
press a button.

**Out, on any surface in the first two rows**: aphorism, inverted sentences, rhetorical contrast of
the *X is not Y, it is Z* shape, em-dash asides, and any sentence whose subject is the design rather
than the thing the reader is doing.

**`--help` is one of those surfaces.** clap builds it out of the doc comments on the `Cli` structs in
`km-video-fetch/src/main.rs` and `km-video-downloader/src/lib.rs`, so a `///` there is read by an
operator and not only by a maintainer. Reasoning goes in a `//` comment above it, where the `///`
keeps the plain statement.

**Plain does not mean shorter.** A fact a reader acts on survives the rewrite: that a browser cannot
hand a page a folder path is why the picker lists server-side, and it stays. What goes is the
argument around the fact.
