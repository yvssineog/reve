pub mod decompile;
pub mod functions;
pub mod listing;
pub mod symbols;

use gpui_kit::component::resizable::{ResizablePanel, resizable_panel};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, Theme, h_flex, v_flex};
use gpui_kit::*;

use crate::icon;
use crate::palette::Palette;

// A pane: a header row (Phosphor duotone icon in the pane's hue + title + optional detail)
// over the caller's body.
pub fn pane(
    theme: &Theme,
    glyph: &str,
    hue: Hsla,
    title: &'static str,
    detail: Option<SharedString>,
) -> ResizablePanel {
    resizable_panel().flex_col().child(
        h_flex()
            .h(px(32.))
            .flex_none()
            .px_3()
            .gap_2()
            .text_xs()
            .font_medium()
            .text_color(theme.muted_foreground)
            .child(icon(glyph, px(14.)).text_color(hue))
            .child(title)
            // Detail reads quieter than the title, in the same font and size so baselines line up.
            .children(detail.map(|detail| {
                div()
                    .min_w_0()
                    .truncate()
                    .font_normal()
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child(detail)
            })),
    )
}

// The body of a pane with nothing to show: a quiet note at the header's size, centered,
// wrapping when long.
pub fn note(text: impl Into<SharedString>, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_h_0()
        .justify_center()
        .px_4()
        .text_xs()
        .text_center()
        .text_color(Palette::of(cx).fg_3)
        .child(text.into())
}

// Text for code: 12px monospace on the listing's rows.
pub fn monospace(cx: &App) -> Div {
    div()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(12.))
        .line_height(listing::ROW)
}

// A row that takes presses: the cursor's fill when marked, a faint one under the pointer
// otherwise.
pub fn pressable(row: Stateful<Div>, marked: bool, palette: &Palette) -> Stateful<Div> {
    if marked {
        row.bg(palette.cursor)
    } else {
        row.hover(|row| row.bg(palette.hover))
    }
}

// A fixed-width column on one line, its text cut with an ellipsis at the edge.
pub fn cell(width: Pixels, text: impl Into<SharedString>, hue: Hsla) -> Div {
    div()
        .flex_none()
        .w(width)
        .truncate()
        .text_color(hue)
        .child(text.into())
}
