use std::path::PathBuf;

use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::*;

use crate::Reve;

/// Opens a fully initialized rêve window, optionally starting with a binary.
/// Both the initial window and later windows use this path so they share the
/// same bounds, title bar, appearance observer, and root view setup.
pub fn open(cx: &mut App, initial_binary: Option<PathBuf>) -> gpui_kit::Result<()> {
    gpui_kit::open_window(options(cx), cx, |window, cx| {
        // Name shown in the operating system's window list until a file is loaded.
        window.set_window_title("rêve");

        // Follow the system appearance independently in every native window.
        window
            .observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx))
            .detach();

        cx.new(|cx| Reve::new(window, cx, initial_binary))
    })
    .map(|_| ())
}

/// Standard bounds and hidden title-bar options shared by every rêve window.
fn options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
        window_min_size: Some(size(px(1024.), px(600.))),
        titlebar: Some(TitlebarOptions {
            traffic_light_position: Some(point(px(16.), px(15.))),
            ..TitleBar::title_bar_options()
        }),
        ..TitleBar::window_options()
    }
}
