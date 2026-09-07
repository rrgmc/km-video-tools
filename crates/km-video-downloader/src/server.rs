//! The state, the socket, and the whole HTTP surface in one `router()`.
//!
//! # Everything the page needs is compiled in
//!
//! `include_str!` and an explicit route each — no `ServeDir`, no CDN, no bundler. See
//! `static/README.md` for why, and for the three things htmx will catch you with.
//!
//! # One job at a time, on purpose
//!
//! Two fetches into one folder would race over the same download archive and the same records file,
//! and yt-dlp's own concurrency is what `-N 4` already asks for. A second Fetch while one is running
//! is answered with a sentence rather than queued.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::Router;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};

use crate::job::Job;
use crate::settings::Settings;

/// htmx, vendored. See `static/README.md` for why nothing is fetched from a CDN.
const HTMX_JS: &str = include_str!("../static/htmx.min.js");
/// Its license, served because a vendored dependency's terms travel with it.
const HTMX_LICENSE: &str = include_str!("../static/htmx-LICENSE.txt");
/// This program's stylesheet.
const STYLE_CSS: &str = include_str!("../static/style.css");
/// This program's own script, which is mostly about htmx not swapping a non-2xx response.
const UI_JS: &str = include_str!("../static/ui.js");
/// The tab's mark. An SVG rather than a PNG so it is a text file like everything else here.
const ICON_SVG: &str = include_str!("../static/icon.svg");

/// What the page calls itself.
pub const APP_NAME: &str = "KM Video Downloader";

/// The only proof that `POST /opened` has reached *this* program.
///
/// A second copy started by a double-click finds the port taken and hands its file over rather than
/// dying; the port being taken is not by itself evidence that what is listening is us. So every
/// answer from that endpoint carries this, and one that does not means somebody else's server has
/// the port.
///
/// **On a refusal as well as on a success**, which is what keeps the other copy's report honest:
/// identity and outcome are two facts, and a marker only on success would make *this program said
/// no* indistinguishable from *this port belongs to something else*.
pub const OPENED_MARK: &str = "km-video-downloader/opened";

/// At most this much of an uploaded list of links.
///
/// A URL is about a hundred bytes, so this is tens of thousands of them — generous for a text file
/// and small enough that a mis-picked video file is refused rather than read into memory.
pub const LIST_LIMIT: usize = 512 * 1024;

/// Everything the handlers share.
#[derive(Clone)]
pub struct State {
    inner: Arc<Inner>,
}

struct Inner {
    /// Where the settings file lives.
    data_dir: PathBuf,
    /// The yt-dlp named on the command line, which is for this run and is never written down.
    yt_dlp: Option<PathBuf>,
    /// What is remembered, and what the form last set.
    settings: Mutex<Settings>,
    /// The one job slot.
    job: Mutex<Option<Arc<Job>>>,
    /// The list this run was handed, where a file association or an argument handed it one.
    ///
    /// **Not in [`Settings`], and that is the whole distinction**: settings are what somebody meant
    /// from now on, and this is what they asked for by double-clicking something a minute ago. It
    /// lasts as long as the process and is written down nowhere.
    opened: Mutex<Option<PathBuf>>,
    /// How to make a window notice that the page under it changed.
    ///
    /// **A closure rather than anything from `tao`**, so this module stays free of the window's
    /// crate and a `--no-default-features` build simply never has one to call. `desktop::run`
    /// installs one that wakes its event loop; nothing else does.
    wake: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl State {
    /// Reads what was remembered and builds the state around it.
    #[must_use]
    pub fn new(data_dir: PathBuf, yt_dlp: Option<PathBuf>) -> Self {
        let settings = crate::settings::load(&data_dir);
        Self {
            inner: Arc::new(Inner {
                data_dir,
                yt_dlp,
                settings: Mutex::new(settings),
                job: Mutex::new(None),
                opened: Mutex::new(None),
                wake: Mutex::new(None),
            }),
        }
    }

    /// Where the settings file lives.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }

    /// The yt-dlp to run, where one was named.
    #[must_use]
    pub fn yt_dlp(&self) -> Option<PathBuf> {
        self.inner.yt_dlp.clone()
    }

    /// What is remembered, as of now.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.inner
            .settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Remembers something new, on disk and then in memory.
    ///
    /// **In that order.** A settings file that disagrees with the page is the confusing failure;
    /// a page that disagrees with itself for one request is not.
    pub fn remember(&self, settings: Settings) {
        crate::settings::save(&self.inner.data_dir, &settings);
        *self
            .inner
            .settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
    }

    /// The job, running or lately finished.
    #[must_use]
    pub fn job(&self) -> Option<Arc<Job>> {
        self.inner
            .job
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Claims the one slot, or says why it cannot.
    pub fn start_job(&self, phase: &str) -> Result<Arc<Job>, String> {
        let mut slot = self
            .inner
            .job
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(running) = slot.as_ref()
            && running.view().running
        {
            return Err("a fetch is already running. Wait for it, or stop it first.".to_owned());
        }
        let job = Job::new(phase);
        *slot = Some(Arc::clone(&job));
        Ok(job)
    }

    /// Takes a list somebody opened: an argument on the command line, or a file association.
    ///
    /// **Two things, and no third one.** The folder moves to the list's own folder, because a list
    /// sitting in a folder of songs is about that folder; and the path is remembered so the page can
    /// offer it. Nothing is fetched — opening a document is somebody saying *look at this*, not
    /// *do it*.
    ///
    /// **Gated on `is_file`**, the way [`km_video_core::args::Plan::folders_own_list`] is, and a
    /// path that is not a file changes nothing at all. It cannot be refused out loud: the
    /// GUI-subsystem executable has nowhere to print, so a refusal would be a silent exit — which
    /// is the failure this whole path exists to avoid.
    ///
    /// Returns whether anything changed, which is what tells the caller whether to wake a window.
    pub fn open_list(&self, list: &Path) -> bool {
        if !list.is_file() {
            return false;
        }
        if let Some(folder) = list
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
        {
            let mut settings = self.settings();
            settings.out = folder.display().to_string();
            self.remember(settings);
        }
        *self
            .inner
            .opened
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(list.to_path_buf());
        true
    }

    /// The list this run was handed, where it was handed one.
    #[must_use]
    pub fn opened(&self) -> Option<PathBuf> {
        self.inner
            .opened
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Says how to make the window notice. Called once, by whoever owns one.
    pub fn attach_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self
            .inner
            .wake
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(wake));
    }

    /// Makes the window notice, where there is one. A build without a window does nothing here.
    pub fn wake(&self) {
        if let Some(wake) = self
            .inner
            .wake
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            wake();
        }
    }
}

/// A socket that is listening but not yet serving.
pub struct Bound {
    listener: tokio::net::TcpListener,
    addr: SocketAddr,
}

impl Bound {
    /// Where a person should point a browser.
    #[must_use]
    pub fn url(&self) -> String {
        browsable_url(self.addr)
    }
}

/// The address to show, which is not always the address bound.
///
/// `0.0.0.0` is a way of listening and not a place to visit; a browser sent there does something
/// different on each platform and nothing useful on any.
fn browsable_url(addr: SocketAddr) -> String {
    if addr.ip().is_unspecified() {
        format!("http://127.0.0.1:{}", addr.port())
    } else {
        format!("http://{addr}")
    }
}

/// Takes the socket.
///
/// **Split from [`serve`] so this happens before anything slow.** The banner can then print a real
/// address — including the port the operating system chose, where port 0 was asked for — and a
/// browser opened straight afterwards waits in the accept backlog rather than meeting a refused
/// connection and showing its own error page.
pub async fn bind(addr: SocketAddr) -> Result<Bound> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot listen on {addr}. Is something already using it?"))?;
    let addr = listener.local_addr().context("reading the bound address")?;
    Ok(Bound { listener, addr })
}

/// Serves until something stops it.
pub async fn serve(bound: Bound, state: State) -> Result<()> {
    axum::serve(bound.listener, router(state).into_make_service())
        .await
        .context("serving the page")
}

/// One embedded text file, with its content type said out loud.
fn embedded(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, content_type)], body)
}

/// The whole HTTP surface.
///
/// GET routes point at `views`, POST routes at `handlers`, without exception — the split is that
/// views render and handlers act.
pub fn router(state: State) -> Router {
    Router::new()
        .route("/", get(crate::views::index))
        .route("/browse", get(crate::views::browse))
        .route("/progress", get(crate::views::progress))
        .route("/results", get(crate::views::results))
        .route("/out", post(crate::handlers::set_out))
        .route("/opened", post(crate::handlers::opened))
        .route(
            "/fetch",
            post(crate::handlers::start).layer(axum::extract::DefaultBodyLimit::max(LIST_LIMIT)),
        )
        .route("/fetch/stop", post(crate::handlers::stop))
        .route(
            "/static/htmx.min.js",
            get(|| async { embedded("application/javascript; charset=utf-8", HTMX_JS) }),
        )
        .route(
            "/static/htmx-LICENSE.txt",
            get(|| async { embedded("text/plain; charset=utf-8", HTMX_LICENSE) }),
        )
        .route(
            "/static/style.css",
            get(|| async { embedded("text/css; charset=utf-8", STYLE_CSS) }),
        )
        .route(
            "/static/ui.js",
            get(|| async { embedded("application/javascript; charset=utf-8", UI_JS) }),
        )
        .route(
            "/static/icon.svg",
            get(|| async { embedded("image/svg+xml; charset=utf-8", ICON_SVG) }),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    fn state() -> State {
        State::new(
            std::env::temp_dir().join("km-video-downloader-router-test"),
            None,
        )
    }

    async fn get_status(path: &str) -> StatusCode {
        router(state())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response")
            .status()
    }

    /// No socket is bound anywhere in this module's tests, and that is deliberate rather than
    /// incidental: on Windows a test binary's path carries a build hash, so a test that listens
    /// raises a fresh firewall prompt on every rebuild and leaves a dead rule behind.
    #[tokio::test]
    async fn the_page_and_its_fragments_answer() {
        assert_eq!(get_status("/").await, StatusCode::OK);
        assert_eq!(get_status("/progress").await, StatusCode::OK);
        assert_eq!(get_status("/results").await, StatusCode::OK);
        assert_eq!(get_status("/browse").await, StatusCode::OK);
    }

    /// The whole of `static/` is inside the executable, and a route that answers 404 for one of
    /// them is a page that renders with no style and no controls.
    #[tokio::test]
    async fn every_static_file_is_embedded_and_not_empty() {
        for (path, at_least) in [
            ("/static/htmx.min.js", 10_000),
            ("/static/htmx-LICENSE.txt", 100),
            ("/static/style.css", 1_000),
            ("/static/ui.js", 100),
            ("/static/icon.svg", 50),
        ] {
            let response = router(state())
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("a request"),
                )
                .await
                .expect("a response");
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("a body");
            assert!(body.len() >= at_least, "{path} is {} bytes", body.len());
        }
    }

    /// A list handed over by a second copy is taken, and the folder follows it.
    ///
    /// **Both halves of `open_list`, and the negative one matters as much.** The endpoint is what a
    /// double-click reaches when a window is already open, so a path that is not there must change
    /// nothing at all rather than move somebody's output folder to a place with no list in it.
    #[tokio::test]
    async fn a_list_handed_over_is_taken_and_the_folder_follows_it() {
        let dir = std::env::temp_dir().join(format!(
            "km-video-downloader-opened-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let songs = dir.join("songs");
        std::fs::create_dir_all(&songs).expect("a scratch folder");
        let list = songs.join(km_video_core::args::BATCH_NAME);
        std::fs::write(&list, "https://example.invalid/a\n").expect("a list");

        let state = State::new(dir.join("data"), None);
        assert_eq!(state.opened(), None);

        let post = |state: State, path: &Path| {
            let body = format!(
                "path={}",
                crate::handoff::encode(&path.display().to_string())
            );
            router(state).oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/opened")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .expect("a request"),
            )
        };

        let response = post(state.clone(), &list).await.expect("a response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("a body");
        assert_eq!(
            std::str::from_utf8(&body).expect("text"),
            OPENED_MARK,
            "the marker is what proves to the other copy that this one is us"
        );
        assert_eq!(state.opened().as_deref(), Some(list.as_path()));
        assert_eq!(state.settings().out, songs.display().to_string());

        // A path that is not a file changes neither.
        let absent = songs.join("not-there.kmvf");
        let response = post(state.clone(), &absent).await.expect("a response");
        assert_ne!(response.status(), StatusCode::OK);
        assert_eq!(state.opened().as_deref(), Some(list.as_path()));
        assert_eq!(state.settings().out, songs.display().to_string());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `0.0.0.0` is a way of listening, not a place to visit.
    #[test]
    fn an_unspecified_address_is_shown_as_loopback() {
        assert_eq!(
            browsable_url("0.0.0.0:8181".parse().expect("an address")),
            "http://127.0.0.1:8181"
        );
        assert_eq!(
            browsable_url("127.0.0.1:8181".parse().expect("an address")),
            "http://127.0.0.1:8181"
        );
    }

    #[test]
    fn a_second_fetch_is_refused_while_one_is_running() {
        let state = state();
        let first = state.start_job("starting").expect("the slot was free");
        let refused = state.start_job("starting").expect_err("one is running");
        assert!(refused.contains("already running"), "{refused}");

        // ...and the slot frees itself when the job ends, rather than needing to be cleared.
        first.done_with("done");
        assert!(state.start_job("starting").is_ok());
    }
}
