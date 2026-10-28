use std::ops::Range;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use reve_model::{Address, Function};

use super::listing::{GUTTER, Listing};
use super::{cell, monospace, pressable};
use crate::palette::Palette;

// The height of every row.
const ROW: Pixels = px(22.);
// Fixed column widths: name and size. Location is the listing's address column.
const NAME: Pixels = px(220.);
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
        self.listing.update(cx, |listing, cx| {
            listing.go_to(entry, ScrollStrategy::Center, cx)
        });
        window.focus(&self.listing.focus_handle(cx), cx);
    }

    fn row(
        &self,
        ix: usize,
        function: &Function,
        listing: &Listing,
        palette: &Palette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let entry = function.entry;
        let signature = function
            .decompilation
            .as_ref()
            .map(|decompilation| decompilation.signature.clone())
            .unwrap_or_default();
        let row = h_flex().id(ix).w_full().h(ROW).px(GUTTER).gap_4();
        pressable(row, entry == self.current, palette)
            .child(cell(
                NAME,
                function.name.clone(),
                palette.name(&function.name),
            ))
            .child(cell(
                listing.address_width(),
                listing.hex(entry),
                palette.fg_3,
            ))
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
        let listing = self.listing.read(cx);
        let (count, location) = (listing.program().functions.len(), listing.address_width());
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
                    .child(column(location, "Location"))
                    .child(div().flex_1().child("Signature"))
                    .child(column(SIZE, "Size").text_right()),
            )
            .child(
                monospace(cx)
                    .text_color(palette.fg)
                    .child(
                        uniform_list(
                            "functions",
                            count,
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                let palette = Palette::of(cx);
                                let listing = this.listing.read(cx);
                                let functions = &listing.program().functions;
                                range
                                    .map(|ix| this.row(ix, &functions[ix], listing, &palette, cx))
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
