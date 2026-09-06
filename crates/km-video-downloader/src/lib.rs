//! A page for fetching karaoke videos.
//!
//! # What it is
//!
//! `km-video-fetch` with a window: set a folder once, paste the links or pick a file of them, press
//! Fetch, and watch it happen. It runs the same fetch — literally the same
//! [`km_video_core::fetch::fetch`] — and differs only in what it does with the events, which is to
//! draw a bar instead of printing lines.
//!
//! # A window, over a page
//!
//! It is an application with a window of its own, and the window is a webview pointed at the
//! loopback address this same process is serving. **One set of templates answers for the window and
//! for a browser tab alike**, which is the arrangement `km-package-builder`, `km-remote` and
//! `km-admin` all use, and it is what stops there being two front ends to keep in step.
//!
//! `--browser` asks for the tab instead, and a build without the `desktop` feature only has the tab.
//!
//! # It listens on loopback
//!
//! Because it writes files as whoever ran it, into whatever folder it is pointed at, and there is no
//! password on it. `--lan` makes it reachable from the rest of the house and says so out loud when
//! it does.

pub mod browse;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod handlers;
pub mod job;
pub mod opener;
pub mod server;
pub mod settings;
pub mod views;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// The port nothing else in this family uses.
///
/// 8177 is the karaoke machine, 8178 its package builder, 8179 the singer's remote and 8180
/// `km-admin`. This is the next one, and the adjacency is the convention rather than a coincidence:
/// somebody with two of these running should be able to guess the second port from the first.
pub const DEFAULT_PORT: u16 = 8181;

/// The folder the settings file lives in, under the platform's own config directory.
pub(crate) const APP_DIR: &str = "km-video-downloader";

/// Which of the two executables this is.
///
/// **The subsystem is a header field fixed at link time**, so a Windows program cannot decide at run
/// time whether it has a console. That is why there are two binaries rather than a flag, and this is
/// how the shared `run` is told which one it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// `km-video-downloader` — a window where the build has one, and no console on Windows.
    Windowed,
    /// `km-video-downloader-console` — never a window, always somewhere to print.
    Console,
}

/// Says one line to whoever is listening.
///
/// **Not `println!`, and that is not a style preference — it is a crash.** `std::io::_print` *panics*
/// on a write failure, with `failed printing to stdout`, and a GUI-subsystem executable on Windows
/// has a null standard output handle that fails every write. Left as `println!`, every double-click
/// would abort the process, and it would never once fail when run from a shell, which is where it
/// would have been tested.
///
/// The error is dropped rather than reported, because there is by definition nowhere to report it.
pub fn say(line: &str) {
    use std::io::Write as _;

    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Fetch karaoke videos from a page instead of a command line.
#[derive(Debug, Parser)]
#[command(name = "km-video-downloader", version, about, long_about = None)]
pub struct Cli {
    /// The port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Listen on every interface, not only this computer.
    ///
    /// There is no password on this program and it writes files as you. Only on a network you
    /// trust, and only while you need it.
    #[arg(long)]
    pub lan: bool,

    /// Open a browser at the page once it is listening.
    ///
    /// Implied where there is no window to open — a build without the `desktop` feature, or a run
    /// with `--browser` or `--lan`.
    #[arg(long)]
    pub open: bool,

    /// Use a browser tab rather than this program's own window.
    #[arg(long)]
    pub browser: bool,

    /// Where the remembered folder and options are kept.
    #[arg(long, value_name = "PATH")]
    pub data_dir: Option<PathBuf>,

    /// The yt-dlp to run, when it is not on the PATH.
    ///
    /// For this run only, and never written down — the same rule `--data-dir` follows, and for the
    /// same reason: an argument is what somebody asked for today, and a settings file is what they
    /// meant from now on.
    #[arg(long, value_name = "PATH")]
    pub yt_dlp: Option<PathBuf>,

    /// Say more. Twice for a great deal more.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Cli {
    /// Where to listen.
    ///
    /// **Loopback unless asked otherwise**, which is also what keeps Windows quiet: a program
    /// listening on a non-loopback address raises the firewall's "allow this app to communicate"
    /// dialog once per program, per port, per profile.
    #[must_use]
    pub fn bind(&self) -> SocketAddr {
        let host = if self.lan {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        SocketAddr::new(host, self.port)
    }

    /// What to log, unless `RUST_LOG` says otherwise.
    #[must_use]
    pub fn log_filter(&self) -> String {
        if let Ok(filter) = std::env::var("RUST_LOG") {
            return filter;
        }
        match self.verbose {
            0 => "km_video_downloader=info".to_owned(),
            1 => "km_video_downloader=debug".to_owned(),
            _ => "km_video_downloader=trace,axum=debug,tower=debug".to_owned(),
        }
    }
}

/// Whether this run will have a window of its own.
///
/// **Three ways not to**, and each is somebody's choice rather than a failure: the build has no
/// window in it, `--browser` asked for a tab, or `--lan` means the page is meant to be reached from
/// another machine — where a window on *this* one is beside the point.
fn will_have_a_window(shell: Shell, cli: &Cli) -> bool {
    matches!(shell, Shell::Windowed) && cfg!(feature = "desktop") && !cli.browser && !cli.lan
}

/// Whether a browser should be opened.
///
/// `--open` asks for one; having no window of our own means one is the only way anybody sees the
/// page, so it is implied rather than required.
fn will_open_a_browser(shell: Shell, cli: &Cli) -> bool {
    !will_have_a_window(shell, cli) && (cli.open || matches!(shell, Shell::Windowed))
}

/// Starts the program.
pub fn run(shell: Shell) -> Result<()> {
    let cli = Cli::parse();
    let data_dir = resolve_data_dir(&cli)?;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(cli.log_filter()))
        .init();

    // **Built by hand rather than through `#[tokio::main]`**, because `tao`'s event loop owns the
    // main thread and never gives it back. The runtime is handed to the window, which holds it for
    // the life of the process; dropping it would stop the server the window is looking at.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;

    let state = server::State::new(data_dir, cli.yt_dlp.clone());

    // **Bound before anything else**, so the address said below is the one that is actually
    // listening — including the port the operating system chose, where 0 was asked for — and so a
    // window or a browser opened a line later waits in the accept backlog rather than meeting a
    // refusal and drawing its own error page.
    let bound = runtime.block_on(server::bind(cli.bind()))?;
    let url = bound.url();

    say(&format!("{} is at {url}", server::APP_NAME));
    if cli.lan {
        say(
            "Serving on every interface. There is no password on this program and it writes files \
             as you.",
        );
    }

    let serving = runtime.spawn(server::serve(bound, state));

    if will_open_a_browser(shell, &cli)
        && let Err(error) = opener::open(&url)
    {
        // Never fatal. The address has been said already; a browser that would not start is a
        // sentence, not a reason to stop serving.
        say(&format!("could not open a browser: {error:#}"));
    }

    #[cfg(feature = "desktop")]
    if will_have_a_window(shell, &cli) {
        // Never returns.
        return desktop::run(&url, runtime);
    }

    runtime
        .block_on(serving)
        .context("the server task ended unexpectedly")?
}

/// Where the settings file goes.
fn resolve_data_dir(cli: &Cli) -> Result<PathBuf> {
    if let Some(dir) = &cli.data_dir {
        return Ok(dir.clone());
    }
    let dirs = directories::ProjectDirs::from("", "", APP_DIR)
        .context("no config directory on this platform; pass --data-dir")?;
    Ok(dirs.config_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The default that keeps Windows Firewall quiet, asserted rather than assumed.
    #[test]
    fn it_listens_on_loopback_unless_asked_otherwise() {
        let cli = Cli::try_parse_from(["km-video-downloader"]).expect("parses");
        assert_eq!(cli.bind().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(cli.bind().port(), DEFAULT_PORT);

        let cli = Cli::try_parse_from(["km-video-downloader", "--lan", "--port", "9000"])
            .expect("parses");
        assert_eq!(cli.bind().ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(cli.bind().port(), 9000);
    }

    #[test]
    fn every_documented_flag_parses() {
        let cli = Cli::try_parse_from([
            "km-video-downloader",
            "--port",
            "8281",
            "--lan",
            "--open",
            "--data-dir",
            "somewhere",
            "--yt-dlp",
            "/opt/bin/yt-dlp",
            "-vv",
        ])
        .expect("parses");
        assert_eq!(cli.port, 8281);
        assert!(cli.lan && cli.open);
        assert_eq!(cli.data_dir, Some(PathBuf::from("somewhere")));
        assert_eq!(cli.yt_dlp, Some(PathBuf::from("/opt/bin/yt-dlp")));
        assert_eq!(cli.verbose, 2);
    }
}
