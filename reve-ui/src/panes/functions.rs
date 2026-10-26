use std::ops::Range;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Function};

use super::listing::{GUTTER, Listing};
use crate::palette::Palette;

// The height of every row.
const ROW: Pixels = px(22.);
// Fixed column widths: name, location (8 hex digits plus 2), size.
const NAME: Pixels = px(220.);
const LOCATION: Pixels = px(72.);
const SIZE: Pixels = px(48.);

// Every function in address order with its location, signature and size. Pressing one takes
// every pane to it; the one the listing cursor is in stays marked and in view.
pub struct Functions {
    listing: Entity<Listing>,
    // The marked function's entry.
    current: Address,
    scroll: UniformListScrollHandle,
    _sync: Subscription,
}

impl Functions {
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        Self {
            current: listing.read(cx).function().entry,
            _sync: cx.observe(&listing, |this, _, cx| this.sync(cx)),
            listing,
            scroll: UniformListScrollHandle::new(),
        }
    }

    // Marks the listing cursor's function, scrolled into view once the cursor leaves the one
    // marked.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let listing = self.listing.read(cx);
        let entry = listing.function().entry;
        if entry == self.current {
            return;
        }
        self.current = entry;
        let functions = &listing.program().functions;
        if let Ok(ix) = functions.binary_search_by_key(&entry, |function| function.entry) {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    // Goes to the function at `entry` and hands the keyboard to the listing, so arrows walk
    // its code next.
    fn pick(&mut self, entry: Address, window: &mut Window, cx: &mut Context<Self>) {
        self.listing
            .update(cx, |listing, cx| listing.go_to(entry, cx));
        window.focus(&self.listing.focus_handle(cx), cx);
    }

    fn row(
        &self,
        ix: usize,
        function: &Function,
        palette: &Palette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let entry = function.entry;
        let name = if function.name.starts_with("FUN_") {
            palette.fg_3
        } else {
            palette.symbol
        };
        let signature = function
            .decompilation
            .as_ref()
            .map(|decompilation| decompilation.signature.clone())
            .unwrap_or_default();
        h_flex()
            .id(ix)
            .h(ROW)
            .px(GUTTER)
            .gap_4()
            .map(|row| {
                if entry == self.current {
                    row.bg(palette.cursor)
                } else {
                    row.hover(|row| row.bg(palette.fg.opacity(0.04)))
                }
            })
            .child(cell(NAME, function.name.clone(), name))
            .child(cell(LOCATION, format!("{:08x}", entry.0), palette.fg_3))
            .child(div().flex_1().min_w_0().truncate().child(signature))
            .child(cell(SIZE, function.size.to_string(), palette.fg_3).text_right())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| this.pick(entry, window, cx)),
            )
            .into_any_element()
    }
}

impl Render for Functions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::of(cx);
        let count = self.listing.read(cx).program().functions.len();
        let column = |width, label| div().flex_none().w(width).child(label);

        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .h(px(24.))
                    .px(GUTTER)
                    .gap_4()
                    .border_b_1()
                    .border_color(palette.hairline)
                    .text_size(px(11.5))
                    .text_color(palette.fg_3)
                    .child(column(NAME, "Name"))
                    .child(column(LOCATION, "Location"))
                    .child(div().flex_1().child("Signature"))
                    .child(column(SIZE, "Size").text_right()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(palette.fg)
                    .child(
                        uniform_list(
                            "functions",
                            count,
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                let palette = Palette::of(cx);
                                let functions = &this.listing.read(cx).program().functions;
                                range
                                    .map(|ix| this.row(ix, &functions[ix], &palette, cx))
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .size_full()
                        .pb_10(),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
    }
}

// A fixed-width column, its text cut with an ellipsis at the edge.
fn cell(width: Pixels, text: impl Into<SharedString>, hue: Hsla) -> Div {
    div()
        .flex_none()
        .w(width)
        .truncate()
        .text_color(hue)
        .child(text.into())
}
