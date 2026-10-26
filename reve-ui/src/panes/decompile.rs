mod mock;

use std::ops::Range;

use gpui_kit::base::actions::{Confirm, SelectDown, SelectPageDown, SelectPageUp, SelectUp};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::Address;

use super::listing::{CONTEXT, GUTTER, HalfPageDown, HalfPageUp, Listing, PAGE, ROW, Spot};
use crate::palette::Palette;

// What a token is, as Ghidra's decompiler marks it; picks its color.
#[derive(Clone, Copy, PartialEq)]
pub enum Syntax {
    Keyword,
    Type,
    Function,
    External,
    Constant,
    String,
    Comment,
}

impl Syntax {
    fn hue(self, palette: &Palette) -> Hsla {
        match self {
            Syntax::Keyword => palette.mnemonic,
            Syntax::Type => palette.fg_3,
            Syntax::Function => palette.symbol,
            Syntax::External => palette.external,
            Syntax::Constant => palette.number,
            Syntax::String => palette.string,
            Syntax::Comment => palette.comment,
        }
    }
}

// A line of C. Its spans run in order; the text between them is plain.
pub struct Line {
    text: SharedString,
    spans: Vec<(Range<usize>, Syntax)>,
}

impl Line {
    // The function names on the line, in order: its calls, or a signature's own name.
    fn names(&self) -> impl Iterator<Item = &str> {
        self.spans
            .iter()
            .filter(|(_, syntax)| matches!(syntax, Syntax::Function | Syntax::External))
            .map(|(range, _)| &self.text[range.clone()])
    }
}

pub struct Function {
    name: &'static str,
    // The line of the signature, after any plate comment.
    signature: usize,
    lines: Vec<Line>,
}

// The function the listing cursor is in, decompiled, kept in step with the listing both
// ways: the listing's cursor picks the line, and a line picked here moves the listing.
pub struct Decompile {
    listing: Entity<Listing>,
    functions: Vec<Function>,
    // The one shown, by index into `functions`.
    function: Option<usize>,
    // The highlighted line.
    cursor: Option<usize>,
    // A line picked here, held while the listing cursor stays on that address.
    picked: Option<(Address, usize)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _sync: Subscription,
}

impl Decompile {
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let sync = cx.observe(&listing, |this, _, cx| this.sync(cx));
        let mut this = Self {
            listing,
            functions: mock::functions(),
            function: None,
            cursor: None,
            picked: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            _sync: sync,
        };
        this.sync(cx);
        this
    }

    // The function shown, for the pane header.
    pub fn name(&self) -> Option<&'static str> {
        self.function.map(|ix| self.functions[ix].name)
    }

    fn lines(&self) -> &[Line] {
        self.function.map_or(&[], |ix| &self.functions[ix].lines)
    }

    // Shows the listing cursor's function, opened at the top, with the cursor's line in view.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let spot = self.listing.read(cx).spot();
        let function = self
            .functions
            .iter()
            .position(|function| function.name == spot.function);
        if function != self.function {
            self.function = function;
            self.scroll
                .0
                .borrow()
                .base_handle
                .set_offset(Point::default());
        }
        self.cursor = match self.picked {
            Some((address, line)) if address == spot.address => Some(line),
            _ => {
                self.picked = None;
                function.and_then(|ix| line_of(&self.functions[ix], &spot))
            }
        };
        if let Some(line) = self.cursor {
            self.scroll.scroll_to_item(line, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    // Moves the listing to the first call on line `ix` it has, then holds the line while the
    // listing stays there. A line with no call holds where the listing already is.
    fn pick(&mut self, ix: usize, cx: &mut Context<Self>) {
        // A press can reach a frame drawn before the function changed.
        let Some(line) = self.lines().get(ix) else {
            return;
        };
        line.names().any(|name| {
            self.listing
                .update(cx, |listing, cx| listing.pick_call(name, cx))
        });
        self.picked = Some((self.listing.read(cx).spot().address, ix));
        self.cursor = Some(ix);
        cx.notify();
    }

    // Follows the call to one of ours at byte `at` of line `ix`. Library calls go nowhere.
    fn follow(&mut self, ix: usize, at: usize, cx: &mut Context<Self>) {
        let Some(line) = self.lines().get(ix) else {
            return;
        };
        let Some((range, _)) = line
            .spans
            .iter()
            .find(|(range, syntax)| *syntax == Syntax::Function && range.contains(&at))
        else {
            return;
        };
        let name = &line.text[range.clone()];
        self.listing.update(cx, |listing, cx| {
            if listing.pick_call(name, cx) {
                listing.follow(cx);
            }
        });
    }

    // Line `ix` as one run of styled text. Pressing picks the line; a double click follows
    // the call under the pointer.
    fn line(&self, ix: usize, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let line = &self.lines()[ix];
        let text = StyledText::new(line.text.clone()).with_highlights(
            line.spans
                .iter()
                .map(|(range, syntax)| (range.clone(), syntax.hue(palette).into())),
        );
        let layout = text.layout().clone();

        div()
            .id(ix)
            .min_w_full()
            .h(ROW)
            .px(GUTTER)
            .map(|row| {
                if self.cursor == Some(ix) {
                    row.bg(palette.cursor)
                } else {
                    row.hover(|row| row.bg(palette.fg.opacity(0.04)))
                }
            })
            .child(text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.pick(ix, cx)),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() == 2
                    && let Ok(at) = layout.index_for_position(event.position())
                {
                    this.follow(ix, at, cx);
                }
            }))
            .into_any_element()
    }
}

impl Render for Decompile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.lines();
        // The list is as wide as its longest line, so it scrolls sideways to that line's end.
        let longest = (0..lines.len()).max_by_key(|&ix| lines[ix].text.len());
        let count = lines.len();

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            // The listing's keys move the listing's cursor, and this pane follows it.
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| {
                this.listing.update(cx, |listing, cx| listing.step(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| {
                this.listing.update(cx, |listing, cx| listing.step(1, cx))
            }))
            .on_action(cx.listener(|this, _: &SelectPageUp, _, cx| {
                this.listing
                    .update(cx, |listing, cx| listing.step(-PAGE, cx))
            }))
            .on_action(cx.listener(|this, _: &SelectPageDown, _, cx| {
                this.listing
                    .update(cx, |listing, cx| listing.step(PAGE, cx))
            }))
            .on_action(cx.listener(|this, _: &HalfPageDown, _, cx| {
                this.listing
                    .update(cx, |listing, cx| listing.half_page(1, cx))
            }))
            .on_action(cx.listener(|this, _: &HalfPageUp, _, cx| {
                this.listing
                    .update(cx, |listing, cx| listing.half_page(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                this.listing.update(cx, |listing, cx| listing.follow(cx))
            }))
            .flex_1()
            .min_h_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.))
            .line_height(ROW)
            .whitespace_nowrap()
            .text_color(Palette::of(cx).fg)
            .child(
                uniform_list(
                    "decompile",
                    count,
                    cx.processor(|this, range: Range<usize>, _, cx| {
                        let palette = Palette::of(cx);
                        range.map(|ix| this.line(ix, &palette, cx)).collect()
                    }),
                )
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(longest)
                .track_scroll(&self.scroll)
                .size_full()
                .pt_1()
                .pb_10(),
            )
            .scrollbar(&self.scroll, ScrollbarAxis::Both)
    }
}

// The line the instruction at `spot` maps to: its call's, or the signature for the entry.
fn line_of(function: &Function, spot: &Spot) -> Option<usize> {
    match spot.call {
        Some(call) => function
            .lines
            .iter()
            .position(|line| line.names().any(|name| name == call)),
        None => spot.entry.then_some(function.signature),
    }
}

#[cfg(test)]
mod tests {
    use super::{line_of, mock};
    use crate::panes::listing::spot;

    #[test]
    fn listing_lands_on_lines() {
        let program = crate::mock::program();
        let functions = mock::functions();
        for function in &program.functions {
            let decompiled = functions.iter().find(|f| f.name == function.name);
            let decompiled = decompiled.unwrap_or_else(|| panic!("{}", function.name));
            for instruction in &function.instructions {
                let spot = spot(&program, function, instruction);
                let line = line_of(decompiled, &spot);
                if spot.entry {
                    assert_eq!(line, Some(decompiled.signature), "{} entry", spot.function);
                    let signature = &decompiled.lines[decompiled.signature];
                    assert_eq!(signature.names().next(), Some(decompiled.name));
                }
                if let Some(call) = spot.call {
                    assert!(line.is_some(), "{call} in {}", spot.function);
                }
            }
        }
    }
}
