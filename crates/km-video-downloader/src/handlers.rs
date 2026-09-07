//! What the controls do.
//!
//! The split with [`crate::views`] is that views render and handlers act. Everything here is a POST,
//! and every refusal is `(StatusCode, String)` carrying a sentence written for a person — because
//! **htmx does not swap a non-2xx response**, so a handler that answers 500 with a perfect
//! explanation puts nothing on the page at all. `ui.js` catches those and shows the sentence; that
//! is most of why it exists.

use std::path::PathBuf;

use axum::extract::{Multipart, State as AxumState};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use km_video_core::{args, fetch, list};

use crate::browse;
use crate::server::{OPENED_MARK, State};

/// `POST /out` — set the folder the files go into.
///
/// Answers with the whole page rather than a fragment, because changing the folder changes four
/// things on it at once: whether the folder exists, how many videos are in it, whether it carries
/// its own list of links, and what the browser is showing.
pub async fn set_out(AxumState(state): AxumState<State>, body: String) -> Response {
    let fields = Fields::parse(&body);
    let Some(typed) = fields.one("path") else {
        return refused("No folder given.");
    };
    let path = browse::tidy(&typed);
    if path.as_os_str().is_empty() {
        return refused("No folder given.");
    }

    let mut settings = state.settings();
    settings.out = path.display().to_string();
    state.remember(settings);

    crate::views::render_page(&state)
}

/// `POST /opened` — this program was opened again, handed over by the copy that could not start.
///
/// **The port is the handoff.** A second copy launched by a double-click finds this one already
/// listening and, rather than exiting without a word, posts here and stops. This is the instance
/// with the window, so this is the instance that answers.
///
/// **A list is the optional half of that message.** With one, this takes it, moves the folder to
/// that list's folder, and wakes the window so the page is redrawn showing it. Without one — the
/// plain second launch, and the commoner of the two — there is nothing to take and the waking *is*
/// the answer.
///
/// **It grants nothing new.** Anything that can reach this port can already post `/out` and
/// `/fetch` and make this program write files wherever it likes; that is what being an
/// unauthenticated server on loopback has always meant here. One more endpoint on that surface is
/// not one more capability.
///
/// Every answer carries [`crate::server::OPENED_MARK`], the refusals included, which is how the copy
/// handing over tells this program from whatever else might have taken the port — and tells *this
/// program said no* from *that port is somebody else's*.
pub async fn opened(AxumState(state): AxumState<State>, body: String) -> Response {
    let fields = Fields::parse(&body);

    // **No path is a message and not a malformed one.** A copy that could not have the port hands
    // over whatever it was opened with, and most of the time that is nothing: somebody opened this
    // program while it was already open. The answer to that is the window, which is here — so come
    // forward and say so, exactly as a list does once it has been taken.
    //
    // An empty value counts as none, matching the filter `lib::opened_list` puts on the other side.
    let path = fields.one("path").filter(|path| !path.trim().is_empty());
    let Some(path) = path else {
        state.wake();
        return OPENED_MARK.into_response();
    };

    let list = browse::tidy(&path);
    if !state.open_list(&list) {
        return refused(&format!(
            "{OPENED_MARK}: there is no file at {}",
            list.display()
        ));
    }
    state.wake();

    OPENED_MARK.into_response()
}

/// `POST /fetch` — start one.
///
/// Multipart, because one of the ways to say what to fetch is a file, and a form that carries a file
/// is multipart whether or not one was chosen.
pub async fn start(AxumState(state): AxumState<State>, multipart: Multipart) -> Response {
    let form = match Form::read(multipart).await {
        Ok(form) => form,
        Err(why) => return refused(&why),
    };

    let mut settings = state.settings();
    let Some(out) = settings.out() else {
        return refused("Set a folder for the videos first.");
    };

    // **Checked here rather than left to yt-dlp**, because yt-dlp does not treat an unknown browser
    // as a usage error it refuses up front: it starts, extracts, and then fails on the first video
    // with something that reads like the site said no. A misspelling deserves to be a sentence
    // before anything has been downloaded.
    if let Some(browser) = &form.cookies_from_browser
        && !args::browser_is_known(browser)
    {
        return refused(&format!(
            "\"{browser}\" is not a browser yt-dlp can read cookies from. It knows {}.",
            args::COOKIE_BROWSERS.join(", ")
        ));
    }

    // The options are remembered as they are used rather than through a Save button, which is the
    // only arrangement where what runs and what comes back tomorrow cannot disagree.
    settings.playlist = form.playlist;
    settings.normalize = form.normalize;
    settings.subs = form.subs;
    settings.limit = form.limit;
    settings.cookies_from_browser = form.cookies_from_browser.clone();
    state.remember(settings.clone());

    let entries = form.entries();
    // An empty list is not a refusal on its own: the folder may carry its own, and `fetch` falls
    // back to it and says so. What is a refusal is nothing anywhere, and that is `fetch`'s to say
    // — it knows the file name to name.
    let from_file = if entries.is_empty() {
        None
    } else {
        match write_asked(&out, &entries) {
            Ok(path) => Some(path),
            Err(why) => return refused(&format!("Could not write the list of links: {why:#}")),
        }
    };

    // **The two settings a list may carry that this page has no control for.**
    //
    // Everything else a header can say is drawn onto a box or a field by `views::page`, so the
    // form above already carries it and folding it in again here would put back what somebody had
    // just unticked. `--format` and `--sort` have nowhere to be shown — they are yt-dlp selector
    // syntax, not a checkbox, and the page deliberately does not offer them — so nothing the form
    // sends could be overruled by taking them from the list.
    //
    // The rule, in a sentence: the page owns what it shows, and the list owns the two it does not.
    let asked = [form.own_list.as_deref(), form.opened_list.as_deref()]
        .into_iter()
        .flatten()
        .map(|list| list::settings_of(std::path::Path::new(list)))
        .fold(list::Settings::default(), |mut all, said| {
            all.format = said.format.or(all.format);
            all.sort = said.sort.or(all.sort);
            all
        });

    let archive = (!form.no_archive).then(|| args::Plan::default_archive(&out));
    let request = fetch::Request {
        plan: args::Plan {
            targets: Vec::new(),
            from_file,
            playlist: form.playlist,
            out,
            limit: form.limit,
            archive,
            cookies_from_browser: form.cookies_from_browser,
            subs: form.subs,
            format: asked.format,
            sort: asked.sort,
            dry_run: form.dry_run,
            progress_lines: true,
        },
        yt_dlp: state.yt_dlp(),
        normalize: form.normalize,
        // **The whole reason this exists.** There is no terminal to hand yt-dlp, so its output is
        // read instead and turned into a bar.
        progress: fetch::Progress::Watched,
    };

    let job = match state.start_job("starting") {
        Ok(job) => job,
        Err(said) => return (StatusCode::CONFLICT, said).into_response(),
    };

    // **`spawn_blocking`, not `spawn`.** `fetch` is ordinary blocking code — it waits on a child
    // process and reads a pipe — and running it on an async worker would stall every other request,
    // including the poll that draws the bar.
    let working = std::sync::Arc::clone(&job);
    tokio::task::spawn_blocking(move || {
        let outcome = fetch::fetch(&request, |event| {
            working.absorb(&event);
            // **Where Stop actually takes effect.** The flag the button sets is read here, on the
            // one closure that is already called at every point where stopping is possible.
            if working.stopping() {
                fetch::Flow::Stop
            } else {
                fetch::Flow::Go
            }
        });
        match outcome {
            Ok(outcome) => working.done_with(summarize(&outcome, form.dry_run)),
            Err(error) => working.failed_with(format!("{error:#}")),
        }
    });

    crate::views::job_fragment(&job)
}

/// `POST /fetch/stop` — ask the run to stop after the video it is on.
pub async fn stop(AxumState(state): AxumState<State>) -> Response {
    if let Some(job) = state.job() {
        job.ask_to_stop();
        return crate::views::job_fragment(&job);
    }
    refused("There is nothing running.")
}

/// One sentence, worded for a person, with a status htmx will not swap.
fn refused(said: &str) -> Response {
    (StatusCode::BAD_REQUEST, said.to_owned()).into_response()
}

/// How a finished run is described in one line.
fn summarize(outcome: &fetch::Outcome, dry_run: bool) -> String {
    let count = plural(outcome.fetched, "video", "videos");
    // **Said first, because it changes what every other number means.** A run somebody stopped did
    // not fetch nothing; it fetched what it had reached, and an unfinished download resumes next
    // time from its `.part` file.
    if outcome.stopped {
        return format!("stopped — {count} finished first");
    }
    // **A run where yt-dlp failed and produced nothing must not read as a quiet success**, and it
    // did until somebody watched a 403 be reported as "nothing new — every link was already in the
    // archive". That sentence is true of a folder already up to date and is a lie about a download
    // that could not be made; only `completed` tells them apart.
    if !outcome.completed && outcome.fetched == 0 {
        return "yt-dlp could not fetch anything — what it said is below".to_owned();
    }
    match (outcome.fetched, dry_run) {
        (0, true) => "nothing to fetch".to_owned(),
        (0, false) => {
            "nothing new — every link was already in the archive, or none produced a file"
                .to_owned()
        }
        (_, true) => format!("would fetch {count}"),
        (_, false) if !outcome.completed => {
            format!("fetched {count}, and yt-dlp reported a failure — see below")
        }
        (_, false) if outcome.all_playable => format!("fetched {count}"),
        _ => format!("fetched {count}, and at least one cannot be played as it is"),
    }
}

/// `1 video` / `2 videos`.
fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// Writes the links out for yt-dlp to read with `--batch-file`.
///
/// **A file rather than arguments**, and not for tidiness: a playlist pasted in as three hundred
/// links is an argv well past what Windows will accept, and this is the one place a page can hand
/// over that many at once. It lands in the output folder under a name a person will recognise if an
/// interrupted run ever leaves one behind.
///
/// **Scratch, and taken away again when the fetch is over.** `fetch::fetch` removes it — see the
/// `Scratch` there, which knows this name. It used not to, and a folder of songs collected one of
/// these per download.
///
/// **Written through [`list::write`]**, so a line that said which it was or where it went says so
/// again in the file. The page parses the markers only to carry them: what acts on them is
/// [`fetch::fetch`], reading this file back. One path in, rather than a second way to say the same
/// thing that only the page would use and only the page would keep working.
fn write_asked(out: &std::path::Path, entries: &[list::Entry]) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(out)?;
    let path = out.join(args::ASKED_NAME);
    list::write(&path, entries)?;
    Ok(path)
}

/// One of the two lists the form names by path rather than by contents.
///
/// **Unreadable is empty rather than an error**, which is [`list::read`]'s own rule: the file may
/// have been deleted between the page being drawn and Fetch being pressed, and a list nobody can
/// read says nothing.
fn read_list(path: Option<&str>) -> String {
    path.and_then(|list| std::fs::read_to_string(list).ok())
        .unwrap_or_default()
}

/// Everything the Fetch form carries.
#[derive(Debug, Default)]
struct Form {
    /// The textarea, one link per line.
    typed: String,
    /// The picked file's contents, where one was picked.
    picked: String,
    /// Whether to take the folder's own list as well.
    own_list: Option<String>,
    /// Whether to take the list this run was opened with as well.
    opened_list: Option<String>,
    playlist: bool,
    normalize: bool,
    subs: bool,
    no_archive: bool,
    dry_run: bool,
    limit: Option<u32>,
    cookies_from_browser: Option<String>,
}

impl Form {
    /// Reads the multipart body.
    ///
    /// An unticked checkbox is simply absent from a form post, which is why every flag here is set
    /// by seeing its name rather than by reading its value.
    async fn read(mut multipart: Multipart) -> Result<Self, String> {
        let mut form = Self::default();

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| format!("could not read the form: {error}"))?
        {
            let name = field.name().unwrap_or_default().to_owned();
            let text = field
                .text()
                .await
                .map_err(|error| format!("could not read `{name}`: {error}"))?;

            match name.as_str() {
                "urls" => form.typed = text,
                // **The file's contents, never its path.** A browser does not give a page the
                // location of a picked file, and does not need to: the lines are the whole point.
                "list" => form.picked = text,
                "own_list" => form.own_list = (!text.trim().is_empty()).then_some(text),
                // A path again rather than contents, for `own_list`'s reason: this file was named
                // by a double-click on this machine, so the server can simply read it.
                "opened_list" => form.opened_list = (!text.trim().is_empty()).then_some(text),
                "playlist" => form.playlist = true,
                "normalize" => form.normalize = true,
                "subs" => form.subs = true,
                "no_archive" => form.no_archive = true,
                "dry_run" => form.dry_run = true,
                "limit" => form.limit = text.trim().parse().ok(),
                "cookies_from_browser" => {
                    form.cookies_from_browser =
                        (!text.trim().is_empty()).then(|| text.trim().to_owned());
                }
                _ => {}
            }
        }

        Ok(form)
    }

    /// Every link the form named, from whichever of the four ways it was given.
    ///
    /// All four at once is allowed and is not a mistake somebody should be told off for: pasting
    /// two links beside a picked file of forty means forty-two, and duplicates are dropped rather
    /// than fetched twice.
    ///
    /// **`list::merge` rather than a `dedup` here**, and the difference is not cosmetic:
    /// `Vec::dedup` drops only *consecutive* equals, so a link present in both the textarea and the
    /// picked file used to survive it and be fetched twice — the sentence above was not true.
    ///
    /// **The order is the order of precedence**, because merging keeps a link's first mention and
    /// the marker that came with it: what somebody typed or picked on the page just now, then the
    /// list they opened this program with, then what the folder says about itself. Each is a
    /// stronger statement of intent than the one after it.
    fn entries(&self) -> Vec<list::Entry> {
        let opened = read_list(self.opened_list.as_deref());
        let own = read_list(self.own_list.as_deref());
        list::merge(&[&self.typed, &self.picked, &opened, &own])
    }
}

/// A urlencoded form body, parsed by hand.
///
/// Two fields at most and one of them a Windows path, which `serde_urlencoded` handles fine — this
/// is here because `set_out` wants to tell "the field was empty" from "the field was not sent", and
/// a `Deserialize` struct flattens both into the same `String`.
struct Fields(Vec<(String, String)>);

impl Fields {
    fn parse(body: &str) -> Self {
        Self(
            body.split('&')
                .filter_map(|pair| pair.split_once('='))
                .map(|(key, value)| (decode(key), decode(value)))
                .collect(),
        )
    }

    fn one(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

/// Percent-decoding, plus `+` for a space.
///
/// `pub(crate)` for [`crate::handoff`], which writes the other half of this and tests the pair
/// against each other — two implementations that disagree is the failure worth pinning, and neither
/// one alone can show it.
pub(crate) fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    // A stray `%` in a path is likelier than a broken encoder, and dropping it
                    // would silently change the path rather than fail.
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A marker typed into the box has to reach the file yt-dlp is pointed at, or the page can say
    /// less than the command line can.
    #[test]
    fn a_marked_line_survives_the_textarea_and_the_picked_file_alike() {
        let form = Form {
            typed: "--playlist https://example.invalid/list\n".to_owned(),
            picked: "--out anime https://example.invalid/a\n".to_owned(),
            ..Form::default()
        };
        let entries = form.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].expand, Some(true));
        assert_eq!(entries[1].out.as_deref(), Some("anime"));
    }

    /// The regression. `Vec::dedup` drops only *consecutive* equals, so a link pasted into the box
    /// and also present in the picked file was fetched twice — which the doc comment on `entries`
    /// has always said it was not.
    #[test]
    fn a_link_in_two_places_is_fetched_once_and_keeps_its_first_marker() {
        let form = Form {
            typed: "--playlist https://example.invalid/a\nhttps://example.invalid/b\n".to_owned(),
            picked: "https://example.invalid/a\n".to_owned(),
            ..Form::default()
        };
        let entries = form.entries();
        assert_eq!(entries.len(), 2, "asked for once");
        assert_eq!(entries[0].expand, Some(true), "the first mention wins");
    }

    /// The list somebody opened is merged in, and it loses to what they did on the page just now.
    ///
    /// **The order in `entries` is a precedence and not an arrangement.** A double-click is a
    /// minute old by the time Fetch is pressed; a line typed into the box is a second old. Merging
    /// keeps a link's first mention *and the marker that came with it*, so the order decides which
    /// `--playlist` wins for a link named twice.
    #[test]
    fn a_list_opened_by_a_double_click_loses_to_what_was_typed_over_it() {
        let dir = std::env::temp_dir().join(format!(
            "km-video-downloader-opened-entries-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");

        let opened = dir.join("anime.kmvf");
        std::fs::write(
            &opened,
            "--playlist https://example.invalid/a\nhttps://example.invalid/c\n",
        )
        .expect("a list");

        let form = Form {
            typed: "--no-playlist https://example.invalid/a\n".to_owned(),
            opened_list: Some(opened.display().to_string()),
            ..Form::default()
        };
        let entries = form.entries();
        assert_eq!(entries.len(), 2, "the opened list is read and merged");
        assert_eq!(
            entries[0].expand,
            Some(false),
            "what was typed on the page beats what was double-clicked"
        );

        // A list that has gone missing between the page being drawn and Fetch being pressed says
        // nothing, rather than failing a fetch that has other links in it.
        std::fs::remove_file(&opened).expect("remove it");
        assert_eq!(form.entries().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The form body carries a Windows path, and percent-decoding it wrongly is how a folder ends
    /// up half set.
    #[test]
    fn a_windows_path_survives_the_form_body() {
        let fields = Fields::parse("path=D%3A%5Ctunes%5Ckaraoke&other=1");
        assert_eq!(fields.one("path").as_deref(), Some(r"D:\tunes\karaoke"));
        assert_eq!(fields.one("absent"), None);
    }

    #[test]
    fn a_space_arrives_as_a_plus_or_as_a_percent() {
        assert_eq!(decode("my+videos"), "my videos");
        assert_eq!(decode("my%20videos"), "my videos");
        assert_eq!(decode("100%"), "100%", "a stray percent is not a failure");
    }

    #[test]
    fn a_run_is_described_in_one_line() {
        let outcome = |fetched, all_playable| fetch::Outcome {
            fetched,
            completed: true,
            all_playable,
            stopped: false,
        };

        // The one this was got wrong on first: a 403 with nothing downloaded is not a folder that
        // was already up to date, and saying so was a quiet lie.
        let mut failed = outcome(0, true);
        failed.completed = false;
        assert!(summarize(&failed, false).contains("could not fetch anything"));
        let mut partly = outcome(2, true);
        partly.completed = false;
        assert!(summarize(&partly, false).contains("reported a failure"));
        assert_eq!(summarize(&outcome(1, true), false), "fetched 1 video");
        assert_eq!(summarize(&outcome(3, true), false), "fetched 3 videos");
        assert!(summarize(&outcome(2, false), false).contains("cannot be played"));
        assert_eq!(summarize(&outcome(0, true), true), "nothing to fetch");
        assert_eq!(summarize(&outcome(2, true), true), "would fetch 2 videos");

        // A stopped run says so first, whatever else was true of it.
        let mut stopped = outcome(1, true);
        stopped.stopped = true;
        assert_eq!(
            summarize(&stopped, false),
            "stopped — 1 video finished first"
        );
    }
}
