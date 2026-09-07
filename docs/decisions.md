# Decisions

Why things here are the way they are. A decision is authoritative over anything else in this
repository that disagrees with it; where the two conflict, the other one is out of date.

## The repository exists so that the karaoke app has no downloader in it

`km-video-fetch` was `tools/cmd/km-video-fetch` inside the karaoke app and is the one thing there
that reached the network for song material.

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

## The packaging profile is copied, and the karaoke app's is authoritative

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

A second implementation of what the karaoke app's `km_video::probe` does, deliberately.

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

In the karaoke app the whole check-and-normalize half sat behind a default-off `video` feature,
because reaching the profile meant linking ffmpeg. A build without the feature downloaded exactly as
well and simply could not say whether what landed was playable.

With `ffprobe` there is nothing to gate, so **the check is never behind a feature** and the tool's
third reason to exist is always available.

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

**A list that says nothing is handed over unread and unrewritten** — byte for byte the argv this tool
has always built. That is what keeps a `km-video-fetch.kmvf` somebody maintains by hand from being
rewritten behind their back, and what keeps a list carrying things `list.rs` does not model working
exactly as it did. An unreadable list says nothing and is yt-dlp's to complain about.

**Single-video runs first, then playlist runs.** A folder's archive makes the first run win a
duplicate, and *this one video* is the more specific statement than *this playlist that happens to
contain it*; it is also the fast half, so somebody watching sees their named picks land before a
two-hundred-item playlist starts. Fixed rather than derived from the flag, because an order that
depended on a checkbox would be worse to reason about and worse to test.

**Each destination keeps its own archive.** `ARCHIVE_NAME` already says an archive is a fact about
*this folder* — which songs are in it — carried with it if the folder is copied elsewhere. So a video
asked for in two folders lands in both, which is what asking for two folders meant.

Four things about yt-dlp that the split made load-bearing, each read out of its `--help` or its
behaviour rather than assumed:

- **`--print-to-file` appends.** So the record file is removed *before* each run and read-and-deleted
  *after* it. The shorter arrangement happens to work today; this one is correct either way.
- **`--batch-file` is an ordinary path argument**, resolved against the working directory and neither
  trimmed nor sanitised — `ARCHIVE_NAME`'s class, not `RECORDS_NAME`'s. So a generated list is passed
  with its folder on it, and the record file is passed as a bare name, and one test asserts both
  halves so the asymmetry cannot be half-remembered.
- **`#`, `;` and `]` all start a comment** in a batch file. Only `#` was skipped before. Passing a `;`
  line through untouched cost nothing; *re-emitting* it as a URL would be an extraction error for a
  line nobody meant to fetch, and a marked list is written back out.
- **A marked file is this tool's file, not yt-dlp's.** yt-dlp would read a marker as a comment or as
  part of the URL. Which is the strongest reason an unmarked list is never rewritten.

**`Event::Downloaded` stays one per fetch**, carrying the combined haul. A second would overwrite a
caller's total with the last run's count alone — and where that run fetched nothing, `job.rs` reads
zero as *nothing known yet* and leaves the bar sweeping for the whole checking phase of a run that
succeeded. The `Command` event does fire per run, which is the narration being a narration.

**`fetch` could not previously tell that a watched run had been stopped.** `run::spawn_watched` folds
a stop into success on purpose — a killed child exits unsuccessfully, and reporting that as a failure
would tell somebody who pressed Stop that yt-dlp had broken. With one run it did not matter, because
the answer arrived with the `Downloaded` event and the web UI's flag is latched. With several it
would have been luck, so the line sink captures the `Flow` it returns. With `|=`, not `=`: collected
stderr is replayed through the sink *after* the kill.

**A line may not name a folder outside the one the fetch was pointed at**, and the rule is one
sentence covering every shape: every component must be an ordinary name. That turns down `..`, a
leading separator, a drive letter and a UNC prefix without naming any of them. Refused before yt-dlp
starts, for the reason the cookie-browser check is: a fault that is knowable up front should not
surface as something that reads like the site said no. Refused rather than quietly clamped, because
`--out ../songs` in a list copied between two machines writes into whatever happens to sit beside the
destination on the second one.

**Three words and not "whatever yt-dlp takes".** A line that could carry a format selector or a
cookie browser is a much larger promise, and each one would be another axis a run has to be split
along.

## A list has an extension of its own, and something opens it

`km-video-fetch.txt` became **`km-video-fetch.kmvf`**, and the extension is registered with
`km-video-downloader` by both setup programs.

**`.txt` was the one thing that could not be said about it.** A list is a document with a grammar —
`--playlist`, `--no-playlist` and `--out folder` in front of a URL, all of it in [`list`] — and an
operating system has no way to learn that from a name it shares with every other text file. So the
file could not be double-clicked, carried no icon, and sat in a file manager as one more `.txt`. The
extension is not decoration on the rename; it *is* the rename.

**Two rules, and they are easy to conflate.** The list a folder carries for itself is matched by
exact filename, in the destination folder, only where nothing else was named — `folders_own_list` is
unchanged but for the constant it joins. The association is matched by extension, on any such file
anywhere, and means only that somebody opened one. Neither rule reaches the other: opening
`anything.kmvf` does not make it a folder's own list, and a folder's own list is found whether or not
anything on the machine associates the extension.

**Renamed with no fallback.** A folder carrying the old name stops being found until it is renamed by
hand. The alternative was two names to document, a precedence between them, and a sentence in every
explanation of the feature for the life of the program — for a tool built here and handed to somebody
as a folder or an installer, that is a worse trade than one rename.

**And nothing this program writes for itself carries the extension.** `ASKED_NAME`, `SINGLES_NAME`
and `PLAYLISTS_NAME` keep `.txt`, and a test pins it. Those sit in somebody's folder of songs for the
length of a run, and an interrupted run leaves them there for good; a program that wrote a
double-clickable file into a folder of songs would be offering to reopen its own workings.

## Opening a list fills the page in, and a second copy hands its own over

`km-video-downloader` takes one positional argument, which is all a shell association passes. It does
two things and no third: the output folder moves to that list's own folder, and the list is offered
on the page, ticked. **Nothing is fetched** — opening a document is somebody saying *look at this*,
not *do it*.

**The failure this had to remove is the second double-click.** A double-click starts a new process
every time, and a second one cannot bind the port — on the GUI-subsystem executable, whose standard
error goes nowhere at all, it exited without a word and left no trace anywhere somebody would think
to look. That is the same class of failure as `println!` being a crash, below, and it would have been
found the same way: never from a shell, only from Explorer.

**The port is the whole mechanism.** No named pipe, no lock file, no single-instance mutex — the
copy that is already running is already an HTTP server on a known port, so the copy that cannot start
posts to it and stops. The instance with the window is the instance that answers.

**A list is the optional half of that message, and it took a correction to get there.** The first
version handed over only where a file association had passed a path, so it answered the second
double-click of a *list* and left the second double-click of the *program* — somebody opening this
while it is already open, which is the commoner of the two — dying exactly as described above. The
condition read as a guard and was really a narrowing.

macOS is what made that visible rather than what caused it: a document arrives there as an Apple
Event, so the positional is always empty and the whole branch was unreachable on that platform. The
rule is now the plain one — **a copy that cannot have the port hands over whatever it was opened
with, including nothing at all** — and `POST /opened` reads a body with no path as *come forward*
rather than as a malformed request. With nothing to take, the waking is the entire answer, which is
what the test for it asserts; a 200 that woke nobody would be this program agreeing it had been
opened and then staying behind whatever is in front of it.

**And it says so where anybody can hear it.** A successful handoff goes through `say`, so the console
build tells somebody who typed the command twice why the second one exited without a window, and the
windowed build drops the line for want of anywhere to put it. That is that function's whole job.

**`POST /opened` grants nothing new**, which is the question worth asking of any new endpoint here.
Anything that can reach that port can already post `/out` and `/fetch` and make this program write
files wherever it likes; that is what *it is on loopback, it already writes files wherever it is
pointed, and it runs as whoever started it* has always meant. One more endpoint on that surface is
not one more capability.

**The request is written by hand over a `TcpStream`.** There is no HTTP client in this tree and this
was not the reason to add one: `reqwest` would bring a TLS stack and a second async runtime into a
program whose whole build story is a handful of crates and no C compiler, to talk to a socket on this
same machine.

**Every answer from that endpoint carries a marker, refusals included.** Identity and outcome are two
facts. With the marker only on success, *this program said no* and *that port belongs to something
else* were indistinguishable from the far end, and a path that was not there got reported as a
stranger on the port.

**macOS reaches all of this by a different road and ends in the same place.** That platform delivers
a document to an application as an Apple Event rather than as an argument, so the positional is empty
there even on the launch that opened the file; `tao`'s `Event::Opened` is the arm that answers, and a
second file opened while the application is running is delivered to it by LaunchServices rather than
by a handoff. Windows never sends that event and macOS never sends the argument, so the two are not
alternatives to be chosen between.

Both roads were walked on a Mac before this was believed: a `.kmvf` opened with nothing running, and
a second opened with the window up. The second reuses the running application and never starts a
process to hand anything over, which is the platform doing by itself what the handoff does on
Windows.

**One case on macOS is improved rather than solved, and it is written down here rather than left to
be discovered.** Where the copy holding the port is one LaunchServices does not know about — the bare
executable, or a `cargo run`, rather than the bundle — a double-clicked list starts the bundle as a
second process, and *that* process is the one the Apple Event is addressed to. It exits at the failed
bind, before an event loop exists to receive it, so it hands over with no path: the window comes
forward and the list is not shown. Better than the silent death it used to be, and short of right.

Closing it means running an event loop in a process whose whole job is to exit — construct an
`NSApp`, wait a bounded moment for `Event::Opened`, hand over whatever arrived — for a case that
needs a copy running outside LaunchServices' knowledge, which is a thing developers have and users do
not. Not paid for yet; the shape of the fix is recorded so the decision is a decision.

**The opened list is drawn only where it is not already the folder's own** — which is the usual case,
since opening one moves the folder to where it sits. Two rows would be this program offering the same
file twice, under two names, with two counts to reconcile.

## A startup failure is put on the screen where there is no console to print it into

`main` returns a `Result`, and an `Err` out of it goes to standard error — the right arrangement
everywhere except the one place this program is actually started from. A GUI-subsystem executable has
a null standard error, and an application bundle launched by LaunchServices has one nobody will read.
So the program vanished on startup and left nothing behind, which is the same silence `say` exists
for and the same one the handoff removed for a taken port.

**The handoff covers the common cause and this covers the rest**: a port held by a program that is
*not* this one, a directory that cannot be made, a runtime that will not start. Rare, and each was
until now indistinguishable from the program not existing.

**The check is on the terminal, not on the build.** `Shell::Console`, the `desktop` feature and
`windows_subsystem` are all proxies for *can anybody read what was printed*, and each is wrong
somewhere — the windowed build run from a terminal has a console, and a console build launched from a
file manager has none. `stderr().is_terminal()` asks the real question, so the dialog appears exactly
where the message would otherwise have gone nowhere and never as a second copy of something already
on screen.

**macOS only, and the asymmetry is deliberate.** `osascript` is in the base system and this program
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
out of `Info.plist` with `plutil`. **Out of the Payload, not out of the staging directory**, which is
the whole point of reading anything back: what is proved has to be what somebody receives, and a
correct staged bundle beside a wrong archive is exactly the failure the round trip is for.

Four things are asserted, and each is one that can be wrong on its own: the plist parses at all; the
type the bundle *declares* and the type its document entry *opens* are the same string, a mismatch
being the macOS shape of an extension registered to a ProgId with no command behind it; there is
exactly one filename extension; and `LSHandlerRank` is `Owner` beside a `CFBundleIdentifier` for
LaunchServices to file the declaration under. **The extension is read rather than repeated** — one
place decides it for this platform, and a copy typed into the check would agree with that place right
up until the day it did not. The Windows driver reads its own out of the `.iss` for the same reason.

**And the assertions were watched to fail.** Each was tried against a deliberately broken
`documents()` — a mismatched identifier, a dropped tag specification, an unbalanced tag — because a
check that has never been seen to refuse anything is not known to check anything.

## The bundle states the macOS floor, and two files agree about it

`11.0` — what the wry/tao stack needs — used to live only in `distribution.xml`, which binds a `.pkg`
install and nothing else. But that is one of two ways this is handed over; the other is the staged
folder, and a bundle without `LSMinimumSystemVersion` dropped on an older Mac fails in the dynamic
loader before `main`, which is the same failure with none of the explanation.

So the bundle states it too, out of `common.sh`'s `dist_min_macos`. Neither copy can be derived from
the other — one is an XML attribute `productbuild` reads, the other a plist key LaunchServices reads —
so what is left is what `rust-toolchain.toml` and `Cargo.toml` already do here: state it twice and
refuse a build where the two disagree.

## The macOS installer's conclusion pane is where a GUI-only install reads

`dist_installed_readme macos` is written into the **fetch** component's payload, and that is correct
— it lands in `/usr/local/km-video-tools` beside the program it describes. The consequence is that
somebody who unticks `km-video-fetch` installs no README anywhere and never reads a word of it.

**Not fixed by putting one beside the application.** A loose `README.txt` in `/Applications` is
against the platform, and inside the bundle it is a file nobody opens. The Installer's own conclusion
pane is what a GUI-only install actually reads, so that is where the association is explained — and
the welcome pane says it is coming, since Windows offers it as a visible tick and macOS offers no
moment at all.

## The whole fetch is a library function that narrates

`km_video_core::fetch::fetch(&Request, on_event)` runs the sequence — preflight, argv, spawn, read
back, check, re-encode — and calls back as it goes. `km-video-fetch` renders those events as lines;
`km-video-downloader` renders them as a progress bar and a list.

**This was the rule above being tested for the first time, and it did not hold.** All of that
sequence used to live in `km-video-fetch`'s own `run()`, interleaved with the printlns reporting it —
a perfectly good shape until a second front end wanted the same sequence, at which point it was
unreusable, because a web page cannot call a function that prints.

Two consequences worth stating, because they are what keeps the two faces honest:

- **The events are a narration, not a state machine.** They arrive in the order things happen and
  each is complete in itself. A caller that ignores every one of them still gets the `Outcome`.
- **`km-video-core` decides no wording.** Every sentence a person reads is in the crate that shows
  it, which is why the command line's output could move without changing a word of it.

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

**This reverses the first decision made about it**, which was that a browser tab would be the whole
user interface — on the grounds that km-admin's `desktop` feature wants `km-tray`, `km-console`,
`km-osopen` and `km-logfile`, all the karaoke app's crates. That was true of two of them and wrong
about what it cost: double-clicking the executable opened a console showing an address, which is not
an application. Of the four, the tray is genuinely optional, `opener.rs` replaces one in fifteen
lines, and the console shim is replaced by [`say`] not being `println!` (below).

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
as `println!`, every double-click would abort the process, and it would never once fail when run
from a shell, which is where it would have been tested.

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
handed a terminal on purpose.** Every other call already pipes or nulls its output, so the window was
never showing anybody anything.

This was got wrong once, and the shape of the mistake is worth keeping. The helper and the reasoning
above both existed, applied to `ffmpeg` and `ffprobe`, while the program was still console-subsystem
and it was merely tidy. The commit that made the downloader a real application changed the subsystem
without extending the helper to `run.rs`, so the case the comment described came true in the one
module that did not have it.

## The icon is the same drawing under a fifth palette

Angular bands, a near-black plate, `KM` with a coloured M: the karaoke app's mark, because these
programs are run beside its and belong to it. **A vermilion lead**, chosen by hue distance rather
than by taste: its four sit at 45°, 148°, 196° and 324°, and this is 11° — 34° clear of the
nearest. The first attempt, a cyan at 180°, was 16° from the package builder's blue, which is
exactly the confusion a per-program palette exists to prevent.

The other opening, around 260°, is a violet and is refused: the tile's own ground is a deep violet
and its middle band a magenta, so a violet lead would make the whole icon one hue with nothing to
catch at 16 pixels.

`crates/km-video-downloader/examples/icon.rs` draws it and writes `icon/`. Its geometry is a copy of
that repository's renderer, which reads colours out of `km_display::theme::Theme` and types out of
SDL, neither reachable from here. **The copy can drift and that is fine**: these are different
programs' icons and are supposed to differ.

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
locations, and that is a security property rather than an oversight. There is no folder input at all.
So the listing is done on this side: the server reads a directory, the page draws it, a click asks
for the next one.

That grants nothing new. The program is on loopback, it already writes files wherever it is pointed,
and it runs as whoever started it.

Deliberately **not** a native dialog, which would mean a GUI toolkit on every platform for one
interaction in a program whose interface is otherwise a page.

The picked *file* of links is the other half of the same fact, read the other way round: what the
browser sends is the file's bytes, and the bytes are all that is wanted.

## The Rust toolchain is pinned exactly

**`rust-toolchain.toml` names one `x.y.z` version, and it is the only place the number is decided.**

This entry used to say the opposite, and the reversal is the point. The argument against a pin was
that this is a standalone tool other people compile with whatever their distribution ships, so one
turns "I have Rust installed" into "rustup will now download a second toolchain". That is the right
answer for a library and this is not one — two programs and the crate they share, every member
`publish = false`, built here and handed to somebody as a folder or an installer. Nothing compiles
against it, so the pin is imposed on nobody but whoever works on it.

**What the floating channel cost was not reproducibility in the abstract.** `task lint` is clippy
with `-D warnings`, so a lint introduced upstream on a Tuesday fails a branch that changed nothing
relevant, and "passes locally" means only "passes on whatever this machine last fetched". It also
broke outright: `rust-version` was inherited from the karaoke app's pin at 1.98.1 while `stable` on
the machine was still 1.98.0, and every cargo command in the workspace refused before it compiled
anything. A pin is what makes those two numbers one decision instead of a race.

**The number propagates rather than being repeated.** `.github/workflows/ci.yml` installs with
`rustup toolchain install --no-self-update`, which resolves the file, `components` and all;
`dtolnay/rust-toolchain` is deliberately not used, its `toolchain` input being required and unable to
read the file, so keeping it would mean the version written twice. `Cargo.toml`'s `rust-version` is
the one copy that no format lets us derive, and `tools/dev/check-toolchain-pin.sh` — which
`task check` runs — fails naming a disagreement, a floating channel, or a returning `dtolnay` step.

The cost is that a bump is now a commit rather than a `rustup update`. That is the trade
the karaoke app made first, and its arrangement is what this copies.

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
older one away, and until it exists something has to decide which of the two is *the* staged build.

**Picking the first glob match is the wrong answer, and it fails silently.** That is what
`tools/dist/bin.sh` and `tools/platform/macos/installer.sh` each did, in two private copies of one
`staged_dir` function: they globbed `<app>-*-<triple>`, and a glob expands in sorted order, so with
both versions present they returned 1.7.0. Nothing downstream disagreed. `bin.sh` gathered the old
executables and read the version out of one of *them*; both setup programs then read it out of that
payload in turn. The result was an installer correctly labelled `1.7.0` carrying a build nobody asked
for, on both platforms, with no failure anywhere for anybody to notice — which is the same class of
mistake as a cleaner that reports success while matching nothing.

`dist_staged_dir` in `tools/dist/common.sh` names the folder instead of searching for one, out of
`dist_pkg_version` — one number, because every crate here is `version.workspace = true`.

**This is not a breach of "from a binary, never a manifest."** That rule answers *what is this
artifact*, and it still does: the version printed, the folder named by `tools/dist/cmd.sh`, and the
number on every installer all still come from a binary's own `--version`. The manifest answers a
different question — *which artifact did we mean* — and the two cannot contradict each other, because
the folder's name was built out of that binary's answer in the first place. What changed is only the
failure: a current build that is not staged now says so, where before it was quietly replaced by an
older one.

## `task clean` is a script, because Task's shell has no `rm`

Task runs every command through its own embedded POSIX shell, which has `for`, `case` and parameter
expansion everywhere but no `rm` — that is an external command, and there is no `rm.exe` on Windows
any more than there is a `sed`. So `clean: rm -rf dist`, which stood in `Taskfile.yml`, was

    "rm": executable file not found in $PATH
    task: Failed to run task "clean": exit status 127

from a PowerShell or a `cmd`. It appeared to work only from a Git Bash — the one shell whose `PATH`
lends Task a coreutils it does not otherwise have — so the task was broken in precisely the situation
the `SH` variable exists to serve, and looked fine in the one it does not need to.

All three clean tasks are `tools/dist/clean.sh` now. The Taskfile chooses which script to run, which
is what it already did for staging, and the deleting happens where coreutils exist. Inherited from
the karaoke app, which hit this first and whose `clean:old` this one is a port of.

**`--old` has no special cases, and that is the property to keep.** An entry is removed only when its
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

Inherited from the karaoke app and worth keeping: **no tracked file names a local drive or folder, a
home LAN address, personal hardware, or a person** — not in prose, not in a comment, not as test
data. A sample is invented; a reproduction step names a variable.
