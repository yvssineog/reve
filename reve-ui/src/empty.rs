use std::time::Duration;

use gpui_kit::component::{ActiveTheme as _, TitleBar, h_flex, v_flex};
use gpui_kit::*;

use crate::OpenBinary;
use crate::header::key_badge;
use crate::logo::Logo;
use crate::palette::Palette;

// Where `OpenBinary`'s I key works: only here, so it never shadows a pane's keys.
pub const CONTEXT: &str = "Empty";

// The window before a binary is open: the logo, and under it the one thing to do.
// The bare title bar keeps the window draggable; its hairline is hidden so the
// surface reads as one piece.
pub fn screen(logo: Entity<Logo>, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .size_full()
        .relative()
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
                // Lifted by the title bar's height, so the logo centers on the window.
                .pb(px(42.))
                .child(logo),
        )
        .child(
            h_flex()
                .absolute()
                .bottom(px(40.))
                .left_0()
                .right_0()
                .justify_center()
                .child(import_hint(cx)),
        )
}

// "Press I to import a binary", quiet under the logo; clicking it works too. It fades
// in with the logo.
fn import_hint(cx: &App) -> impl IntoElement {
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
        .child("Press")
        // Monospace, so the I reads as a letter rather than a bar.
        .child(key_badge("I", cx).font_family(cx.theme().mono_font_family.clone()))
        .child("to import a binary")
        .on_click(|_, window, cx| window.dispatch_action(OpenBinary.boxed_clone(), cx))
        .with_animation(
            "import-hint",
            Animation::new(Duration::from_millis(1200)).with_easing(ease_out_quint()),
            |hint, reveal| hint.opacity(reveal),
        )
}
