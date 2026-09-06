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
