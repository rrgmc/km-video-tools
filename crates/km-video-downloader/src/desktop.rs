//! The window. Behind the `desktop` feature, which is on by default.
//!
//! **A viewer over the page this program already serves**, and nothing else. There is no second
//! front end: the webview loads the same loopback URL a browser would, so one set of templates
//! answers for the window and the browser alike, and a bug fixed in one is fixed in both.
//!
//! **`tao`'s event loop owns the main thread and its `run` never returns**, which is the whole
//! reason [`crate::run`] builds its tokio runtime by hand rather than wearing `#[tokio::main]`. The
//! runtime is moved in here and held for the life of the process: dropping it would stop the server
//! the window exists to look at.
//!
//! **Never on Linux.** `wry` links libwebkit2gtk at load time, so a Linux build carrying this
//! feature does not *start* on a machine without it — a failure in the dynamic loader, before
//! `main`, that no flag can rescue. `--no-default-features` is the build for that machine, and
//! `--browser` is the flag for a machine that has the library and would still rather use a tab.

use anyhow::Result;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use crate::server::APP_NAME;

/// The size to open at, before the screen is consulted.
///
/// Tall rather than wide: the page is one column of panels, and what grows down it is the list of
/// what arrived and the log under the bar.
const WANTED: (f64, f64) = (1000.0, 900.0);

/// How much of a screen the window may take when [`WANTED`] does not fit.
///
/// `WANTED` is taller than the *logical* screen on a laptop at 150% scaling — 1920x1080 becomes
/// 1280x720 — and a window larger than the desktop opens with its controls off the edge.
const SCREEN_SHARE: f64 = 0.9;

/// Opens the window and runs the event loop. Never returns.
pub fn run(url: &str, runtime: tokio::runtime::Runtime) -> Result<()> {
    let event_loop: EventLoop<()> = EventLoop::new();
    let window = build_window(&event_loop, url);

    // **Held rather than dropped.** Dropping the runtime stops the server the window is looking at,
    // and the page would go blank on the first navigation.
    let _runtime = runtime;

    // The menu bar, which is macOS's alone and is not decoration there: ⌘Q is a key equivalent on
    // the application menu rather than a key the window is sent, so a shell without one cannot be
    // quit from the keyboard — and its text fields cannot be copied into or pasted from. A failure
    // is a blemish rather than a reason to stop serving.
    let _menu = install_app_menu()
        .inspect_err(
            |error| tracing::debug!(%error, "no menu bar; the program is running all the same"),
        )
        .ok();

    if window.is_none() {
        // A window that could not open is not a reason to stop serving. Say where the page is and
        // let whoever started this open it themselves.
        crate::say(&format!(
            "could not open a window; {APP_NAME} is still serving at {url}"
        ));
    }

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Builds the window and the webview in it, or says why not.
fn build_window(
    event_loop: &EventLoop<()>,
    url: &str,
) -> Option<(tao::window::Window, wry::WebView)> {
    let (size, position) = opening_geometry(event_loop);
    let mut builder = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(size);
    if let Some(position) = position {
        builder = builder.with_position(position);
    }
    let window = match builder.build(event_loop) {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(%error, "could not open a window");
            return None;
        }
    };

    // **The profile is named rather than left to the platform**, or WebView2 puts a
    // `km-video-downloader.exe.WebView2` folder beside the executable — and a folder somebody was
    // handed then starts growing browser profiles inside itself.
    let mut context = wry::WebContext::new(webview_data_dir());

    let webview = match WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        // yt-dlp's own page is a link this page offers, and it does not belong inside this window.
        .with_new_window_req_handler(|url, _features| {
            let _ = crate::opener::open(&url);
            wry::NewWindowResponse::Deny
        })
        .build(&window)
    {
        Ok(webview) => webview,
        Err(error) => {
            tracing::warn!(%error, "could not open a webview");
            return None;
        }
    };

    Some((window, webview))
}

/// The application menu, on the one platform that has one.
///
/// **Predefined items throughout**, each of which takes an optional title override that is not
/// given: the platform's own wording — translated, and with the application's name already in it
/// where AppKit puts one — is better than anything spelled here.
#[cfg(target_os = "macos")]
fn install_app_menu() -> Result<muda::Menu> {
    use muda::{PredefinedMenuItem, Submenu};

    let bar = muda::Menu::new();

    // AppKit takes the bold name from the bundle and ignores this title, but a submenu has to be
    // given one and the program's own name is the least surprising thing to meet in a debugger.
    let app = Submenu::new(APP_NAME, true);
    bar.append(&app)?;
    app.append_items(&[
        &PredefinedMenuItem::about(None, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ])?;

    let edit = Submenu::new("Edit", true);
    bar.append(&edit)?;
    edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ])?;

    bar.init_for_nsapp();
    Ok(bar)
}

/// Everywhere else there is no application menu to put anything in.
///
/// **Windows is one of those places as much as Linux is.** It has window menus, which are a
/// different thing and not wanted here: this window is one page and has no commands of its own.
#[cfg(not(target_os = "macos"))]
fn install_app_menu() -> Result<()> {
    Ok(())
}

/// Where the webview keeps its profile.
///
/// Under this program's own data directory, so a folder somebody was handed does not grow one
/// beside the executable.
fn webview_data_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", crate::APP_DIR)
        .map(|dirs| dirs.data_dir().join("webview"))
}

/// The size to open at — [`WANTED`], or as much of it as the screen will take — and where to put it.
///
/// **Centered rather than left to the platform.** A window with no position asked for lands wherever
/// the platform's cascade puts it, which on Windows is a staircase from the top left that walks
/// further down with every run — and this is a program opened, used and closed again all evening.
///
/// **The monitor's own corner is added**, because a primary monitor is not always at the origin: on
/// a two-screen desk the left-hand one routinely has a negative x, and centering on the *size* alone
/// would put the window on whichever screen happened to contain that coordinate.
///
/// No monitor at all is a real answer rather than an error — a session with no display attached yet
/// — and the honest response is to ask for what was wanted, name no position, and let the platform
/// decide.
fn opening_geometry(
    event_loop: &EventLoop<()>,
) -> (
    tao::dpi::LogicalSize<f64>,
    Option<tao::dpi::LogicalPosition<f64>>,
) {
    let Some(monitor) = event_loop.primary_monitor() else {
        return (tao::dpi::LogicalSize::new(WANTED.0, WANTED.1), None);
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let corner = monitor.position().to_logical::<f64>(scale);
    let (width, height) = fit(WANTED, (screen.width, screen.height));
    let (left, top) = centre((screen.width, screen.height), (width, height));
    (
        tao::dpi::LogicalSize::new(width, height),
        Some(tao::dpi::LogicalPosition::new(
            corner.x + left,
            corner.y + top,
        )),
    )
}

/// [`WANTED`] shrunk to [`SCREEN_SHARE`] of a screen of the given logical size.
///
/// Pure, so the awkward cases can be asserted rather than discovered on somebody's laptop.
fn fit(wanted: (f64, f64), screen: (f64, f64)) -> (f64, f64) {
    (
        wanted.0.min(screen.0 * SCREEN_SHARE),
        wanted.1.min(screen.1 * SCREEN_SHARE),
    )
}

/// The window's offset into a screen of the given size, so that it sits in the middle of it.
///
/// **Never negative.** [`fit`] keeps the window inside the screen, so this cannot go below zero in
/// practice — but a platform reporting a screen smaller than it gives us would otherwise put the
/// window's title bar above the top of the display, where it cannot be dragged back.
fn centre(screen: (f64, f64), window: (f64, f64)) -> (f64, f64) {
    (
        ((screen.0 - window.0) / 2.0).max(0.0),
        ((screen.1 - window.1) / 2.0).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_never_opens_bigger_than_the_screen() {
        // The case this exists for: 1920x1080 at 150% scaling is a 1280x720 *logical* desktop, and
        // a window taller than that opens with its controls off the edge.
        assert_eq!(fit(WANTED, (1920.0, 1080.0)), WANTED);

        let (width, height) = fit(WANTED, (1280.0, 720.0));
        assert!(width <= 1280.0 * SCREEN_SHARE && height <= 720.0 * SCREEN_SHARE);

        // An ultrawide is short rather than narrow, so only the height gives.
        let (width, height) = fit(WANTED, (3440.0, 900.0));
        assert_eq!(width, WANTED.0);
        assert!(height < WANTED.1);
    }

    /// The window opens in the middle, and never off the top left.
    #[test]
    fn a_window_opens_in_the_middle_of_the_screen() {
        assert_eq!(centre((1920.0, 1080.0), (1000.0, 700.0)), (460.0, 190.0));

        // A window exactly the size of the screen sits at the origin rather than at a half-pixel.
        assert_eq!(centre((1280.0, 720.0), (1280.0, 720.0)), (0.0, 0.0));

        // And one the platform says is larger than the screen is clamped rather than pushed off the
        // top, where its title bar could not be reached.
        assert_eq!(centre((800.0, 600.0), (1500.0, 900.0)), (0.0, 0.0));
    }
}
