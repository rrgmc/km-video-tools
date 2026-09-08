//! What the page looks like: the template structs and the GET handlers that fill them in.
//!
//! The split with [`crate::handlers`] is that **views render and handlers act**. A GET here builds a
//! struct out of owned data and hands it to [`render`]; nothing in this file changes anything.

use askama::Template;
use axum::extract::{Query, State as AxumState};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::browse;
use crate::job;
use crate::server::{APP_NAME, State};

/// Renders one template, or says why it could not.
///
/// A render failure is this program's own bug rather than anything a person did, so it answers 500
/// with the reason — which `ui.js` then shows as a toast, because htmx will not swap a non-2xx
/// response and the page would otherwise simply stop responding.
fn render<T: Template>(template: &T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!(%error, "could not render a page");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not render this page: {error}"),
            )
                .into_response()
        }
    }
}

/// The one page.
#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    /// What the program calls itself, in the title and the bar.
    pub app_name: &'static str,
    /// The remembered folder, as typed.
    pub out: String,
    /// Whether that folder is there. A folder that is not there yet is not an error — it is made on
    /// the first fetch — but saying so beats a silent surprise.
    pub out_exists: bool,
    /// How many videos are already in it, where it exists.
    pub out_videos: Option<usize>,
    /// The folder's own list of links, where it has one.
    pub own_list: Option<String>,
    /// How many links that list holds.
    pub own_list_count: usize,
    /// The list this run was opened with, where it was opened with one **and it is not already the
    /// folder's own list**.
    pub opened_list: Option<String>,
    /// How many links that list holds.
    pub opened_list_count: usize,
    /// Expand playlists.
    pub playlist: bool,
    /// Re-encode anything outside the profile.
    pub normalize: bool,
    /// Mux subtitles.
    pub subs: bool,
    /// Fetch what is already in the archive again.
    ///
    /// **The one box that nothing remembers**, and it stays that way: fetching everything twice
    /// is a thing somebody does once for a reason. It is here because a list may ask for it, and
    /// a box that was ticked by the list must show as ticked.
    pub no_archive: bool,
    /// At most this many from a playlist, as typed. Empty for no limit.
    pub limit: String,
    /// A browser to take cookies from, as typed.
    pub cookies_from_browser: String,
    /// The browsers yt-dlp can read cookies out of, for the picker.
    pub browsers: &'static [&'static str],
    /// The job, running or lately finished.
    pub job: Option<job::View>,
    /// What has arrived so far.
    pub results: Vec<job::Arrival>,
}

/// `GET /` — the whole page.
pub async fn index(AxumState(state): AxumState<State>) -> Response {
    render_page(&state)
}

/// The whole page, for a handler that changed enough of it to redraw all of it.
pub fn render_page(state: &State) -> Response {
    render(&page(state))
}

/// The page as it stands.
fn page(state: &State) -> Index {
    let settings = state.settings();
    let out = settings.out();
    let own_list = out
        .as_deref()
        .and_then(km_video_core::args::Plan::folders_own_list);

    // **Drawn only where it is not already the row above.** Opening a list moves the folder to that
    // list's own folder, so the usual case — a `km-video-fetch.kmvf` double-clicked where it sits —
    // ends with the opened file *being* the folder's own list, and the checkbox for it is already
    // there. Two identical rows would be this program announcing the same file twice.
    let opened_list = state
        .opened()
        .filter(|opened| own_list.as_deref() != Some(opened.as_path()));

    // **What a list asks for is drawn onto the controls rather than applied behind them.** It is
    // a stronger statement than anything remembered — somebody wrote it into that folder — but
    // it is still only what will happen unless the person looking changes it, and the page is
    // where they would. `handlers` then reads the form and nothing folds these in twice; see
    // `fetch::Request::apply_list_settings` for that separation.
    //
    // Both lists, in `Form::entries`' order of precedence: the one opened just now beats the
    // one the folder carries.
    let asked = [own_list.as_deref(), opened_list.as_deref()]
        .into_iter()
        .flatten()
        .fold(km_video_core::list::Settings::default(), |mut all, list| {
            let said = km_video_core::list::settings_of(list);
            all.playlist |= said.playlist;
            all.subs |= said.subs;
            all.normalize |= said.normalize;
            all.no_archive |= said.no_archive;
            all.limit = said.limit.or(all.limit);
            all.cookies_from_browser = said.cookies_from_browser.or(all.cookies_from_browser);
            all
        });

    Index {
        app_name: APP_NAME,
        out: settings.out.clone(),
        out_exists: out.as_deref().is_some_and(std::path::Path::is_dir),
        out_videos: out.as_deref().and_then(count_videos),
        own_list_count: own_list
            .as_deref()
            .map_or(0, |list| km_video_core::list::read(list).len()),
        own_list: own_list.map(|list| list.display().to_string()),
        opened_list_count: opened_list
            .as_deref()
            .map_or(0, |list| km_video_core::list::read(list).len()),
        opened_list: opened_list.map(|list| list.display().to_string()),
        playlist: settings.playlist || asked.playlist,
        normalize: settings.normalize || asked.normalize,
        subs: settings.subs || asked.subs,
        no_archive: asked.no_archive,
        limit: asked
            .limit
            .or(settings.limit)
            .map(|n| n.to_string())
            .unwrap_or_default(),
        cookies_from_browser: asked
            .cookies_from_browser
            .clone()
            .or_else(|| settings.cookies_from_browser.clone())
            .unwrap_or_default(),
        browsers: &km_video_core::args::COOKIE_BROWSERS,
        job: state.job().map(|job| job.view()),
        results: state.job().map(|job| job.results()).unwrap_or_default(),
    }
}

/// How many video files a folder already holds.
///
/// The top level only, and by extension only. This is a number shown beside a folder name to help
/// somebody recognise it, not an inventory — walking a corpus of tens of thousands of files to
/// draw one page would be absurd.
fn count_videos(dir: &std::path::Path) -> Option<usize> {
    let entries = std::fs::read_dir(dir).ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v"
                        )
                    })
            })
            .count(),
    )
}

/// The progress fragment, which is also what starting a fetch answers with.
#[derive(Template)]
#[template(path = "_job.html")]
pub struct JobFragment {
    /// The job, or `None` where nothing has run yet.
    pub job: Option<job::View>,
}

/// `GET /progress` — the poll.
pub async fn progress(AxumState(state): AxumState<State>) -> Response {
    render(&JobFragment {
        job: state.job().map(|job| job.view()),
    })
}

/// The same fragment, for a handler that has just started something.
pub fn job_fragment(job: &job::Job) -> Response {
    render(&JobFragment {
        job: Some(job.view()),
    })
}

/// What arrived.
#[derive(Template)]
#[template(path = "_results.html")]
pub struct Results {
    /// One row per file.
    pub results: Vec<job::Arrival>,
}

/// `GET /results` — redrawn once, by the last frame the poll ever receives.
pub async fn results(AxumState(state): AxumState<State>) -> Response {
    render(&Results {
        results: state.job().map(|job| job.results()).unwrap_or_default(),
    })
}

/// One directory listing.
#[derive(Template)]
#[template(path = "_browse.html")]
pub struct Browse {
    /// Where this is and what is in it.
    pub listing: browse::Listing,
}

/// Which folder to list.
#[derive(Debug, Deserialize)]
pub struct Where {
    /// The folder, absent on the first click.
    #[serde(default)]
    pub at: Option<String>,
}

/// `GET /browse` — one step of the folder picker.
pub async fn browse(AxumState(state): AxumState<State>, Query(asked): Query<Where>) -> Response {
    let at = match asked.at {
        // An explicitly empty `at` is the drive list, and is how the crumb bar climbs off `D:\`.
        Some(typed) => browse::tidy(&typed),
        None => browse::start(state.settings().out().as_deref()),
    };
    render(&Browse {
        listing: browse::list(&at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder for one test, gone again afterwards.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "km-video-downloader-views-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// **A list's header is drawn onto the controls, not applied behind them.**
    ///
    /// The reason this is a test rather than an implementation detail: `handlers` builds the
    /// request from the form and folds nothing in a second time, so if these boxes did not come
    /// up ticked the list would be silently ignored — and if anything folded them in twice, a box
    /// somebody unticked would come back. Both failures look like the page working.
    #[test]
    fn a_lists_header_comes_up_on_the_page_as_ticked_boxes() {
        let dir = scratch("header");
        let songs = dir.join("songs");
        std::fs::create_dir_all(&songs).expect("a folder of songs");

        let own = songs.join(km_video_core::args::BATCH_NAME);
        std::fs::write(
            &own,
            "--normalize\n--subs\n--no-archive\n--limit 25\n--cookies-from-browser firefox\n\nhttps://example.invalid/a\n",
        )
        .expect("a list with a header");

        let state = State::new(dir.join("data"), None);
        assert!(state.open_list(&own));

        let page = page(&state);
        assert!(
            page.normalize,
            "the list asked for it and nothing remembered it"
        );
        assert!(page.subs);
        assert!(page.no_archive);
        assert_eq!(page.limit, "25");
        assert_eq!(page.cookies_from_browser, "firefox");
        assert!(
            !page.playlist,
            "and says nothing about what it did not mention"
        );
        assert_eq!(
            page.own_list_count, 1,
            "the header is not one of the links, which is the bug this would hide"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The list somebody opened is offered once, not twice.
    ///
    /// **This is the usual case rather than an edge.** Opening a list moves the folder to that
    /// list's own folder, so a `km-video-fetch.kmvf` double-clicked where it sits *becomes* the
    /// folder's own list — and the checkbox for that is already on the page. Drawing both would be
    /// this program offering the same file under two names, with two counts to reconcile.
    #[test]
    fn a_list_that_is_also_the_folders_own_is_offered_once() {
        let dir = scratch("own");
        let songs = dir.join("songs");
        std::fs::create_dir_all(&songs).expect("a folder of songs");

        let own = songs.join(km_video_core::args::BATCH_NAME);
        std::fs::write(&own, "https://example.invalid/a\n").expect("a list");

        let state = State::new(dir.join("data"), None);
        assert!(state.open_list(&own));

        let page = page(&state);
        assert_eq!(page.own_list.as_deref(), Some(&*own.display().to_string()));
        assert_eq!(page.own_list_count, 1);
        assert_eq!(
            page.opened_list, None,
            "the row above it already is this file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A list opened from somewhere with a name of its own is a second row, because nothing else on
    /// the page mentions it.
    #[test]
    fn a_list_under_any_other_name_is_a_row_of_its_own() {
        let dir = scratch("other");
        let songs = dir.join("songs");
        std::fs::create_dir_all(&songs).expect("a folder of songs");

        let opened = songs.join("anime-openings.kmvf");
        std::fs::write(
            &opened,
            "https://example.invalid/a\n--playlist https://example.invalid/b\n",
        )
        .expect("a list");

        let state = State::new(dir.join("data"), None);
        assert!(state.open_list(&opened));

        let page = page(&state);
        assert_eq!(
            page.own_list, None,
            "that folder carries no list of its own"
        );
        assert_eq!(
            page.opened_list.as_deref(),
            Some(&*opened.display().to_string())
        );
        assert_eq!(page.opened_list_count, 2);

        // And it renders — the template reads both of these and askama is checked at run time.
        let html = page.render().expect("the page renders");
        assert!(html.contains("anime-openings.kmvf"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
