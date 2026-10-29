use std::path::Path;
use std::time::Duration;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::header::{self, key_badge};
use crate::logo::Logo;
use crate::palette::Palette;
use crate::recent::RecentFile;
use crate::{OpenBinary, OpenRecentBinary, icon};

// Where `OpenBinary`'s I key works; the shortcut does not shadow pane typing.
pub const CONTEXT: &str = "Empty";

// The home screen keeps the existing mark, title bar, palette, and quiet text
// treatment, adding recent files below the primary open action.
pub fn screen(logo: Entity<Logo>, recent_files: Vec<RecentFile>, window: &Window, cx: &App) -> Div {
    let theme = cx.theme();
    let available_height = (window.viewport_size().height - header::HEIGHT).max(px(0.));
    let recent_height = px(320.).min(available_height * 0.34);
    let width = px(520.).min(window.viewport_size().width - px(64.));
    let mut content = v_flex()
        .w(width)
        .max_w_full()
        .items_center()
        .gap(px(8.))
        .child(logo)
        .child(open_hint(cx));
    if !recent_files.is_empty() {
        content = content.child(recent_list(recent_files, width, recent_height, cx));
    }

    v_flex()
        .size_full()
        .relative()
        .bg(theme.background)
        .child(
            TitleBar::new()
                .h(header::HEIGHT)
                .bg(theme.background)
                .border_color(transparent_black())
                .child(
                    h_flex()
                        .ml(header::file_menu_left_inset())
                        .px_1p5()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(header::file_menu(cx)),
                ),
        )
        .child(
            div().flex_1().min_h_0().overflow_y_scrollbar().child(
                v_flex()
                    .w_full()
                    .min_h(available_height)
                    .items_center()
                    .justify_center()
                    .py(px(20.))
                    .child(content),
            ),
        )
}

fn recent_list(
    files: Vec<RecentFile>,
    width: Pixels,
    max_height: Pixels,
    cx: &App,
) -> impl IntoElement {
    let palette = Palette::of(cx);
    let mut rows = v_flex().gap(px(2.));
    for file in files {
        rows = rows.child(recent_row(file, &palette));
    }

    div()
        .w(width)
        .max_h(max_height)
        .overflow_y_scrollbar()
        .border_t_1()
        .border_color(palette.hairline)
        .pt_1()
        .child(rows)
}

fn recent_row(file: RecentFile, palette: &Palette) -> AnyElement {
    let identity = file.path.to_string_lossy().into_owned();
    let location = file
        .path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .display()
        .to_string();
    let name = div()
        .min_w_0()
        .truncate()
        .text_size(px(13.))
        .text_color(palette.fg)
        .child(file.name.clone());
    let location = div()
        .min_w_0()
        .truncate()
        .text_size(px(11.))
        .text_color(palette.fg_3)
        .child(if file.available {
            location
        } else {
            "File missing".to_owned()
        });
    let details = v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(1.))
        .child(name)
        .child(location);
    let details = if file.available {
        let action = OpenRecentBinary {
            path: file.path.clone(),
            in_new_window: false,
        };
        details
            .id(format!("recent-open-{identity}"))
            .cursor_pointer()
            .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            .into_any_element()
    } else {
        details.into_any_element()
    };
    h_flex()
        .id(identity)
        .w_full()
        .h(px(42.))
        .px_2()
        .gap_2()
        .items_center()
        .rounded(px(6.))
        .hover({
            let hover = palette.hover;
            move |row| row.bg(hover)
        })
        .child(icon("folder-simple-duotone", px(16.)).text_color(palette.fg_3))
        .child(details)
        .into_any_element()
}

// A quiet text button that preserves the original home screen's single-action feel.
fn open_hint(cx: &App) -> impl IntoElement {
    let palette = Palette::of(cx);
    h_flex()
        .id("import")
        .gap(px(7.))
        .px_2()
        .py_1()
        .text_size(px(12.5))
        .text_color(palette.fg_3)
        .cursor_pointer()
        .hover(|hint| hint.text_color(palette.fg))
        .child("Open binary")
        .child(key_badge("I", cx).font_family(cx.theme().mono_font_family.clone()))
        .on_click(|_, window, cx| window.dispatch_action(OpenBinary.boxed_clone(), cx))
        .with_animation(
            "import-hint",
            Animation::new(Duration::from_millis(1200)).with_easing(ease_out_quint()),
            |hint, reveal| hint.opacity(reveal),
        )
}
