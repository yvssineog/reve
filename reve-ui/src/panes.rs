pub mod decompile;
pub mod functions;
pub mod listing;
pub mod symbols;

use gpui_kit::component::list::ListItem;
use gpui_kit::component::resizable::{ResizablePanel, resizable_panel};
use gpui_kit::component::tree::TreeEntry;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, Theme, h_flex, v_flex,
};
use gpui_kit::*;

use crate::palette::Palette;

// A pane: a header row (Phosphor duotone icon in the pane's hue + title + optional detail)
// over the caller's body.
pub fn pane(
    theme: &Theme,
    icon: &str,
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
            .child(
                Icon::empty()
                    .path(format!("icons/{icon}-duotone.svg"))
                    .with_size(px(14.))
                    .text_color(hue),
            )
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

// A compact tree row: indent, caret, icon in its hue, the label, then an optional detail on
// the right. When narrow the label gives way first, cut with an ellipsis.
fn tree_row(
    ix: usize,
    entry: &TreeEntry,
    icon: &str,
    hue: Hsla,
    detail: Option<SharedString>,
    cx: &App,
) -> ListItem {
    let theme = cx.theme();
    let caret = match (entry.is_folder(), entry.is_expanded()) {
        (false, _) => None,
        (true, false) => Some("caret-right"),
        (true, true) => Some("caret-down"),
    };

    ListItem::new(ix)
        .h(px(24.))
        .py_0()
        .px(px(6.))
        .rounded(px(6.))
        .text_size(px(13.))
        .child(
            h_flex()
                .gap_1()
                .pl(px(16.) * entry.depth() as f32)
                // Leaves keep an empty caret slot so their icons line up with folders'.
                .child(div().flex_none().size(px(12.)).children(caret.map(|caret| {
                    Icon::empty()
                        .path(format!("icons/{caret}.svg"))
                        .with_size(px(12.))
                        .text_color(theme.muted_foreground)
                })))
                .child(
                    Icon::empty()
                        .path(format!("icons/{icon}.svg"))
                        .with_size(px(14.))
                        .text_color(hue),
                )
                .child(div().min_w_0().truncate().child(entry.item().label.clone()))
                // Faint monospace at the label's size, so baselines line up.
                .children(detail.map(|detail| {
                    div()
                        .ml_auto()
                        .pl_2()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground.opacity(0.7))
                        .child(detail)
                })),
        )
}
