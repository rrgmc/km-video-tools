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

pub mod alert;
pub mod browse;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod handlers;
pub mod handoff;
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
/// 8177 is the karaoke app, 8178 its package builder, 8179 the singer's remote and 8180
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
/// **Not `println!`, which is a crash here.** `std::io::_print` *panics* on a write failure, with
/// `failed printing to stdout`, and a GUI-subsystem executable on Windows has a null standard output
/// handle that fails every write. Left as `println!`, every double-click aborts the process, and it
/// never once fails when run from a shell, which is where it would be tested.
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
    // **What a file association hands over**, and the only argument this program takes
    // positionally, because a shell association passes a path and nothing else. See
    // `km_video_core::args::EXTENSION`.
    //
    // Nothing is fetched on open: a document says *look at this*, not *do it*.
    //
    // **macOS never uses this.** It delivers a document as an Apple Event rather than as an
    // argument, which `desktop.rs` answers with `Event::Opened`.
    /// A list of links to open.
    ///
    /// The output folder moves to the list's own folder, and the list is offered on the page,
    /// already ticked. Nothing is fetched until you press Fetch.
    #[arg(value_name = "PATH")]
    pub list: Option<PathBuf>,

    /// The port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Listen on every interface, not only this computer.
    ///
    /// There is no password on this program and it writes files as you. Only on a network you
    /// trust, and only while you need it.
    #[arg(long)]
    pub lan: bool,

    // Implied by `wants_window` returning false: a build without the `desktop` feature, or a run
    // with `--browser` or `--lan`.
    /// Open a browser at the page once it is listening.
    ///
    /// Implied where this run has no window of its own: a browser-only build, or a run with
    /// `--browser` or `--lan`.
    #[arg(long)]
    pub open: bool,

    /// Use a browser tab rather than this program's own window.
    #[arg(long)]
    pub browser: bool,

    /// Where the remembered folder and options are kept.
    #[arg(long, value_name = "PATH")]
    pub data_dir: Option<PathBuf>,

    // The same rule `--data-dir` follows, and for the same reason: an argument is what somebody
    // asked for today, and a settings file is what they meant from now on.
    /// The yt-dlp to run, when it is not on the PATH.
    ///
    /// For this run only. It is not remembered.
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

/// The list this run was asked to open, tidied the way a typed path is.
///
/// A shell hands an association's argument over with whatever quoting it had, so this goes through
/// the same [`browse::tidy`] the folder field uses rather than being taken as typed.
fn opened_list(cli: &Cli) -> Option<PathBuf> {
    cli.list
        .as_ref()
        .map(|list| browse::tidy(&list.display().to_string()))
        .filter(|list| !list.as_os_str().is_empty())
}

/// Whether a bind failed because something already has the port.
///
/// **Asked of the source rather than of the message.** `server::bind` wraps its error with a
/// sentence for a person, and matching on that sentence would break the day it is reworded.
fn address_is_taken(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io| io.kind() == std::io::ErrorKind::AddrInUse)
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
    let bound = match runtime.block_on(server::bind(cli.bind())) {
        Ok(bound) => bound,
        // **The port being taken is the ordinary way a second double-click arrives**: there is one
        // of these running already, and it is the one with the window. Hand over and stop.
        // Anything else, and any failure to hand over, is an ordinary error.
        //
        // **Whether a list came with it does not enter into this.** Gating the branch on a list
        // answers the second double-click of a list and leaves the second double-click of the
        // program to die without a word, which is the commoner of the two and the whole failure
        // `handoff` exists for. It also makes the branch unreachable on macOS, where a document
        // arrives as an Apple Event and the positional is always empty.
        Err(error) => {
            if address_is_taken(&error) {
                let list = opened_list(&cli);
                handoff::hand_over(cli.port, list.as_deref()).with_context(|| match &list {
                    Some(list) => format!("handing {} to the copy already running", list.display()),
                    None => "asking the copy already running to come forward".to_owned(),
                })?;
                // **Said rather than silent**, for the console build, where somebody typed this and
                // is owed a reason the second one exited without a window. In the windowed build
                // there is nowhere for it to go and `say` drops it, which is that function's
                // whole job.
                say(match &list {
                    Some(_) => "Already running. The list went to the window that is already open.",
                    None => "Already running. That window has been brought forward.",
                });
                return Ok(());
            }
            return Err(error);
        }
    };
    let url = bound.url();

    // A list named on the command line, taken before the page is ever drawn so the first draw
    // already shows it. Silently nothing where the path is not a file — see `State::open_list`.
    if let Some(list) = opened_list(&cli) {
        state.open_list(&list);
    }

    say(&format!("{} is at {url}", server::APP_NAME));
    if cli.lan {
        say(
            "Serving on every interface. There is no password on this program and it writes files \
             as you.",
        );
    }

    // Cloned before the server takes it: `State` is an `Arc` inside, so this is the same state and
    // not a copy of it, and the window needs a handle to be woken through.
    #[cfg(feature = "desktop")]
    let state_for_window = state.clone();
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
        return desktop::run(&url, runtime, state_for_window);
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
    use std::path::Path;

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

    /// The one positional, which is what a file association hands over.
    ///
    /// **A path is not mistaken for a flag and a flag is not mistaken for a path**: a shell
    /// association passes the file and nothing else, and the two must still be told apart when
    /// somebody runs this from a prompt with both. `debug_assert` above covers the definition being
    /// well formed; this covers what it actually parses.
    #[test]
    fn a_list_is_the_one_thing_this_takes_without_a_flag_in_front_of_it() {
        let cli = Cli::try_parse_from(["km-video-downloader"]).expect("parses");
        assert_eq!(
            cli.list, None,
            "nothing is opened unless something is named"
        );

        let opened = r"C:\Users\Someone\My Songs\anime.kmvf";
        let cli = Cli::try_parse_from(["km-video-downloader", opened]).expect("parses");
        assert_eq!(cli.list.as_deref(), Some(Path::new(opened)));

        let cli =
            Cli::try_parse_from(["km-video-downloader", "--browser", "list.kmvf"]).expect("parses");
        assert!(cli.browser);
        assert_eq!(cli.list.as_deref(), Some(Path::new("list.kmvf")));
    }

    /// A bind that failed because the port is taken is told from one that failed for any other
    /// reason, and it is told from the *source* rather than from the sentence wrapped around it.
    ///
    /// This is what decides whether a second double-click hands its list over or reports an error,
    /// and matching on `bind`'s wording would break silently the day that wording improves.
    #[test]
    fn a_taken_port_is_recognised_through_the_sentence_wrapped_around_it() {
        let taken = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::AddrInUse))
            .context("cannot listen on 127.0.0.1:8181. Is something already using it?");
        assert!(address_is_taken(&taken));

        let denied = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            .context("cannot listen on 127.0.0.1:80. Is something already using it?");
        assert!(!address_is_taken(&denied));

        assert!(!address_is_taken(&anyhow::anyhow!("no config directory")));
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
