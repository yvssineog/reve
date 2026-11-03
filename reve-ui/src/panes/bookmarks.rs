use std::ops::Range;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use reve_model::Address;

use super::listing::{GUTTER, Listing};
use super::{cell, monospace, note, pressable};
use crate::palette::Palette;

// The height of every row.
const ROW: Pixels = px(22.);
// Fixed column widths: function and instruction. Location is the listing's address column.
const FUNCTION: Pixels = px(220.);
const INSTRUCTION: Pixels = px(280.);

// Every bookmark in address order with its location, function, instruction and comment.
// Pressing one takes every pane to it; the one under the listing cursor stays marked.
pub struct Bookmarks {
    listing: Entity<Listing>,
    // The listing cursor's address and the listing's edits when last drawn.
    seen: (Address, usize),
    scroll: UniformListScrollHandle,
    _sync: Subscription,
}

impl Bookmarks {
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let seen = {
            let listing = listing.read(cx);
            (listing.address(), listing.edits())
        };
        Self {
            seen,
            _sync: cx.observe(&listing, |this, _, cx| this.sync(cx)),
            listing,
            scroll: UniformListScrollHandle::new(),
        }
    }

    // Draws again once the cursor moves or an edit lands.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let listing = self.listing.read(cx);
        let seen = (listing.address(), listing.edits());
        if seen != self.seen {
            self.seen = seen;
            cx.notify();
        }
    }

    // Goes to the bookmark at `address` and hands the keyboard to the listing.
    fn pick(&mut self, address: Address, window: &mut Window, cx: &mut Context<Self>) {
        self.listing
            .update(cx, |listing, cx| listing.go_to(address, cx));
        window.focus(&self.listing.focus_handle(cx), cx);
    }

    // A bookmark Ghidra put where rêve shows no instruction keeps only its location.
    fn row(
        &self,
        ix: usize,
        address: Address,
        listing: &Listing,
        palette: &Palette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let at = listing.at(address);
        let (function, instruction, comment) = match at {
            Some((function, instruction)) => (
                function.name.as_str(),
                format!("{} {}", instruction.mnemonic, listing.operands(instruction)),
                instruction
                    .comment
                    .as_deref()
                    .map(|comment| comment.lines().next().unwrap_or_default()),
            ),
            None => ("", String::new(), None),
        };
        let row = h_flex().id(ix).w_full().h(ROW).px(GUTTER).gap_4();
        pressable(row, address == self.seen.0, palette)
            .child(cell(
                listing.address_width(),
                listing.hex(address),
                palette.fg_3,
            ))
            .child(cell(FUNCTION, function.to_owned(), palette.name(function)))
            .child(cell(INSTRUCTION, instruction, palette.fg))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(palette.comment)
                    .child(comment.unwrap_or_default().to_owned()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| this.pick(address, window, cx)),
            )
            .into_any_element()
    }
}

impl Render for Bookmarks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::of(cx);
        let listing = self.listing.read(cx);
        let (count, location) = (listing.program().bookmarks.len(), listing.address_width());
        if count == 0 {
            let key = if cfg!(target_os = "macos") {
                "⌘D"
            } else {
                "Ctrl+D"
            };
            return note(
                format!("No bookmarks. {key} bookmarks the cursor's instruction."),
                cx,
            )
            .size_full()
            .into_any_element();
        }
        let column = |width, label| div().flex_none().w(width).child(label);

        v_flex()
            .size_full()
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
                    .child(column(location, "Location"))
                    .child(column(FUNCTION, "Function"))
                    .child(column(INSTRUCTION, "Instruction"))
                    .child(div().flex_1().child("Comment")),
            )
            .child(
                monospace(cx)
                    .flex_1()
                    .min_h_0()
                    .text_color(palette.fg)
                    .child(
                        uniform_list(
                            "bookmarks",
                            count,
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                let palette = Palette::of(cx);
                                let listing = this.listing.read(cx);
                                let bookmarks = &listing.program().bookmarks;
                                range
                                    .map(|ix| this.row(ix, bookmarks[ix], listing, &palette, cx))
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .size_full()
                        .pb_10(),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .into_any_element()
    }
}
