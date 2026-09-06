//! A page for fetching karaoke videos.
//!
//! # What it is
//!
//! `km-video-fetch` with a window: set a folder once, paste the links or pick a file of them, press
//! Fetch, and watch it happen. It runs the same fetch — literally the same
//! [`km_video_core::fetch::fetch`] — and differs only in what it does with the events, which is to
//! draw a bar instead of printing lines.
//!
//! # Why a page and not a window
//!
//! Because a page is the whole user interface, and a browser is on every machine that has yt-dlp.
//! `km-admin`, this program's model, has an optional `desktop` feature that swaps the tab for a
//! `wry` webview in a `tao` window; that is worth having there and is not reachable here, since it
//! also wants a tray, a console shim and an opener that are karaokemachine's crates. What is left is
//! simpler: one binary, a real stdout, and `println!` that does not panic.
//!
//! # It listens on loopback
//!
//! Because it writes files as whoever ran it, into whatever folder it is pointed at, and there is no
//! password on it. `--lan` makes it reachable from the rest of the house and says so out loud when
//! it does.

pub mod browse;
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
const APP_DIR: &str = "km-video-downloader";

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
    #[arg(long)]
    pub open: bool,

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

/// Starts the program.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = resolve_data_dir(&cli)?;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(cli.log_filter()))
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;

    let state = server::State::new(data_dir, cli.yt_dlp.clone());

    // **Bound before anything else**, so the address printed below is the one that is actually
    // listening — including the port the operating system chose, where 0 was asked for — and so a
    // browser opened a line later waits in the accept backlog rather than meeting a refusal.
    let bound = runtime.block_on(server::bind(cli.bind()))?;
    let url = bound.url();

    println!("{} is at {url}", server::APP_NAME);
    if cli.lan {
        println!(
            "Serving on every interface. There is no password on this program and it writes files \
             as you."
        );
    }

    let serving = runtime.spawn(server::serve(bound, state));

    if cli.open
        && let Err(error) = opener::open(&url)
    {
        // Never fatal. The address is on the screen already; a browser that would not start is a
        // sentence, not a reason to stop serving.
        println!("could not open a browser: {error:#}");
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
