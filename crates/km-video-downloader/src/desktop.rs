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
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use crate::server::{APP_NAME, State};

/// What this program sends its own event loop.
///
/// One variant, because there is one thing anything outside the loop ever needs of the window:
/// notice that the page under you changed. It is sent by the closure `run` hands to
/// [`State::attach_wake`], from whichever thread `POST /opened` happened to land on.
#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    /// A list was opened. Redraw and come forward.
    Opened,
}

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
pub fn run(url: &str, runtime: tokio::runtime::Runtime, state: State) -> Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let window = build_window(&event_loop, url);

    // **How anything outside this thread reaches the window.** A copy of this program started by a
    // double-click cannot bind the port, so it posts its list to this one instead; that request is
    // served on a runtime thread, and this is what lets it say so here.
    let proxy = event_loop.create_proxy();
    state.attach_wake(move || {
        // A closed loop means the window is going away, which is not a fault worth reporting.
        let _ = proxy.send_event(UserEvent::Opened);
    });

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
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            // **macOS's only route to a double-clicked file**, and the reason this arm exists at
            // all: that platform delivers a document to a running application as an Apple Event
            // through `application:openURLs:`, never as an argument — so `Cli::list` is empty there
            // even on the launch that opened the file. Windows is the other way round and never
            // sends this.
            Event::Opened { urls } => {
                let opened = urls
                    .iter()
                    .filter_map(|url| url.to_file_path().ok())
                    .filter(|list| state.open_list(list))
                    .count();
                if opened > 0 {
                    show(window.as_ref());
                }
            }

            // The page changed under the window: something handed this program a list. Redrawn
            // rather than patched, because the whole page is what the server renders anyway.
            Event::UserEvent(UserEvent::Opened) => show(window.as_ref()),

            _ => {}
        }
    });
}

/// Redraws the page and brings the window forward.
///
/// **`location.reload()` rather than anything cleverer.** The page is server-rendered and every
/// fragment on it comes from what the server knows, so reloading it *is* the update — the same
/// reason `decisions.md` gives for a page reloaded mid-fetch picking the job back up.
///
/// A failure is a blemish and not a reason to stop: the list has been taken either way, and it is
/// there the next time anybody touches the page.
fn show(window: Option<&(tao::window::Window, wry::WebView)>) {
    let Some((window, webview)) = window else {
        return;
    };
    if let Err(error) = webview.evaluate_script("location.reload()") {
        tracing::debug!(%error, "could not redraw the page");
    }
    window.set_focus();
}

/// Builds the window and the webview in it, or says why not.
fn build_window(
    event_loop: &EventLoop<UserEvent>,
    url: &str,
) -> Option<(tao::window::Window, wry::WebView)> {
    let (size, position) = opening_geometry(event_loop);
    let mut builder = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(size);
    if let Some(position) = position {
        builder = builder.with_position(position);
    }
    builder = with_icons(builder);
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

/// Puts this program's mark on the window.
///
/// **The executable already carries the icon and the window does not get it for free.** `build.rs`
/// writes it into the resource section, which is what Explorer, the Start menu and the taskbar
/// button read *before* the process starts — but a window is created from a class with no icon set,
/// so without this the title bar and the running taskbar button show Windows' default. The picture
/// was right in every place except the one somebody is looking at.
///
/// **Two icons, because Windows keeps two.** `with_window_icon` sets `ICON_SMALL`, which is the
/// title bar, and `with_taskbar_icon` sets `ICON_BIG`, which is the button and Alt-Tab.
///
/// **And the size is asked for rather than left to default, which is the trap.** `from_resource`
/// with no size passes `LR_DEFAULTSIZE`, meaning the *large* metric — so the title bar would get a
/// 32-pixel drawing squashed into 16, which on a mark this small is the difference between two
/// letters and a smudge. The `.ico` carries real 16, 32, 48 and 256 frames; each ask picks one.
#[cfg(windows)]
fn with_icons(builder: WindowBuilder) -> WindowBuilder {
    use tao::platform::windows::WindowBuilderExtWindows as _;

    builder
        .with_window_icon(icon_at(16))
        .with_taskbar_icon(icon_at(256))
}

/// One frame out of the executable's own icon resource.
///
/// `None` never stops the window opening: an icon that could not be loaded is a blemish, and a
/// program that refuses to run over one is a bug.
#[cfg(windows)]
fn icon_at(side: u32) -> Option<tao::window::Icon> {
    use tao::platform::windows::IconExtWindows as _;

    /// Which resource holds the icon: `winresource`'s default application icon id, which is what
    /// `build.rs`'s `set_icon` writes. Named rather than spelled `1` at the call site, because the
    /// two have to agree and nothing checks that they do.
    const ICON_ORDINAL: u16 = 1;

    let wanted = tao::dpi::PhysicalSize::new(side, side);
    match tao::window::Icon::from_resource(ICON_ORDINAL, Some(wanted)) {
        Ok(icon) => Some(icon),
        Err(error) => {
            tracing::debug!(%error, side, "no icon in this executable; Windows' default it is");
            None
        }
    }
}

/// Everywhere else the platform already has the picture.
///
/// macOS reads the bundle's `CFBundleIconFile`, which `tools/dist/cmd.sh` writes; Linux never builds
/// this feature.
#[cfg(not(windows))]
fn with_icons(builder: WindowBuilder) -> WindowBuilder {
    builder
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
    event_loop: &EventLoop<UserEvent>,
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
