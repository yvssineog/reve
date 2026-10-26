use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, TitleBar, h_flex, v_flex,
};
use gpui_kit::*;

use crate::OpenFolder;
use crate::header::key_badge;
use crate::palette::Palette;

// Where `OpenFolder`'s I key works: only here, so it never shadows a pane's keys.
pub const CONTEXT: &str = "Empty";

// The window before a folder is open: one action on the content tint, nothing else.
// The bare title bar keeps the window draggable; its hairline is hidden so the
// surface reads as one piece.
pub fn screen(cx: &App) -> Div {
    let theme = cx.theme();
    let palette = Palette::of(cx);
    v_flex()
        .size_full()
        .bg(theme.background)
        .child(
            TitleBar::new()
                .h(px(42.))
                .bg(theme.background)
                .border_color(transparent_black()),
        )
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                // Lifted by the title bar's height, so the group centers on the window.
                .pb(px(42.))
                .child(
                    Icon::empty()
                        .path("icons/folder-simple-duotone.svg")
                        .with_size(px(28.))
                        .text_color(palette.number),
                )
                .child(
                    div()
                        .mt_3()
                        .text_size(px(13.))
                        .font_medium()
                        .text_color(palette.fg)
                        .child("No project open"),
                )
                .child(open_folder(cx).mt_5()),
        )
}

// The "Go to" field's shape and fills: same height, radius and badge.
fn open_folder(cx: &App) -> Button {
    let ink = cx.theme().foreground;
    Button::new("open-folder")
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(ink.opacity(0.64))
                .hover(ink.opacity(0.14))
                .active(ink.opacity(0.18)),
        )
        .bg(ink.opacity(0.1))
        .h_7()
        .p_0()
        .rounded(px(10.))
        .child(
            h_flex()
                .gap_3()
                .pl(px(11.))
                .pr(px(5.))
                .text_size(px(12.5))
                .child("Open Folder…")
                // Monospace, so the I reads as a letter rather than a bar.
                .child(key_badge("I", cx).font_family(cx.theme().mono_font_family.clone())),
        )
        .on_click(|_, window, cx| window.dispatch_action(OpenFolder.boxed_clone(), cx))
}
