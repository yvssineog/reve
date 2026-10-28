use std::ops::Range;

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Decompilation, DecompilationStatus, TokenKind};

use super::listing::{GUTTER, Listing, ROW};
use super::{monospace, note, pressable};
use crate::palette::Palette;

// The function the listing cursor is in, as Ghidra decompiled it, with the lines the cursor's
// instruction became marked.
pub struct Decompile {
    listing: Entity<Listing>,
    // The shown function's entry.
    entry: Address,
    // The cursor's instruction.
    address: Address,
    name: SharedString,
    // Its C, one line each, or why there is none.
    code: Result<Vec<Line>, SharedString>,
    scroll: UniformListScrollHandle,
    _sync: Subscription,
}

// A line of C, the syntax class of each of its spans, and the instructions it came from.
#[derive(Debug, PartialEq)]
struct Line {
    text: SharedString,
    spans: Vec<(Range<usize>, TokenKind)>,
    // Ascending.
    addresses: Vec<Address>,
}

impl Line {
    fn new(line: &reve_model::Line) -> Self {
        let mut text = String::new();
        let mut spans = Vec::new();
        for token in &line.tokens {
            let start = text.len();
            text.push_str(&token.text);
            spans.push((start..text.len(), token.kind));
        }
        Self {
            text: text.into(),
            spans,
            addresses: line.addresses.clone(),
        }
    }

    fn holds(&self, address: Address) -> bool {
        self.addresses.binary_search(&address).is_ok()
    }

    // Where pressing the line takes the cursor: its first instruction, or nowhere when the
    // cursor is on one of its instructions already or it has none.
    fn target(&self, cursor: Address) -> Option<Address> {
        if self.holds(cursor) {
            None
        } else {
            self.addresses.first().copied()
        }
    }
}

impl Decompile {
    // Opens on the listing cursor's function and follows the cursor from there.
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let sync = cx.observe(&listing, |this, _, cx| this.sync(cx));
        let cursor = listing.read(cx);
        let function = cursor.function();
        Self {
            entry: function.entry,
            address: cursor.address(),
            name: function.name.clone().into(),
            code: code(function.decompilation.as_ref()),
            listing,
            scroll: UniformListScrollHandle::new(),
            _sync: sync,
        }
    }

    // The function shown, for the pane header.
    pub fn name(&self) -> SharedString {
        self.name.clone()
    }

    // Shows the listing cursor's function, opened at the top, once the cursor leaves the
    // one shown. Then brings the first line the cursor's instruction became into view.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let listing = self.listing.read(cx);
        let (function, address) = (listing.function(), listing.address());
        if function.entry == self.entry && address == self.address {
            return;
        }
        if function.entry != self.entry {
            self.entry = function.entry;
            self.name = function.name.clone().into();
            self.code = code(function.decompilation.as_ref());
            self.scroll
                .0
                .borrow()
                .base_handle
                .set_offset(Point::default());
        }
        self.address = address;
        let lines = self.code.as_deref().unwrap_or_default();
        if let Some(ix) = lines.iter().position(|line| line.holds(address)) {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    // Moves the listing cursor to where pressing line `ix` points, then hands the keyboard to
    // the listing.
    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.code.as_deref().unwrap_or_default();
        if let Some(address) = lines.get(ix).and_then(|line| line.target(self.address)) {
            self.listing
                .update(cx, |listing, cx| listing.move_to(address, cx));
        }
        window.focus(&self.listing.focus_handle(cx), cx);
    }
}

impl Render for Decompile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = match &self.code {
            Ok(lines) => lines,
            Err(why) => return note(why.clone(), cx).size_full().into_any_element(),
        };
        // The list is as wide as its longest line, so it scrolls sideways to that line's end.
        let longest = (0..lines.len()).max_by_key(|&ix| lines[ix].text.len());

        monospace(cx)
            .size_full()
            .whitespace_nowrap()
            .text_color(Palette::of(cx).fg)
            .child(
                uniform_list(
                    "decompile",
                    lines.len(),
                    cx.processor(|this, range: Range<usize>, _, cx| {
                        let palette = Palette::of(cx);
                        let lines = this.code.as_deref().unwrap_or_default();
                        range
                            .map(|ix| {
                                let line = &lines[ix];
                                let spans = line.spans.iter().filter_map(|(span, kind)| {
                                    Some((span.clone(), hue(*kind, &palette)?.into()))
                                });
                                let text =
                                    StyledText::new(line.text.clone()).with_highlights(spans);
                                // Lines that came from code take a press, like listing rows.
                                div()
                                    .id(ix)
                                    .h(ROW)
                                    .w_full()
                                    .px(GUTTER)
                                    .when(!line.addresses.is_empty(), |row| {
                                        pressable(row, line.holds(this.address), &palette)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _, window, cx| {
                                                    this.pick(ix, window, cx)
                                                }),
                                            )
                                    })
                                    .child(text)
                                    .into_any_element()
                            })
                            .collect()
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
            .into_any_element()
    }
}

// The color Ghidra's syntax class reads in. Types and annotations recede; variables,
// parameters and punctuation keep the pane's ink.
fn hue(kind: TokenKind, palette: &Palette) -> Option<Hsla> {
    match kind {
        TokenKind::Keyword => Some(palette.mnemonic),
        TokenKind::Function => Some(palette.symbol),
        TokenKind::Constant => Some(palette.number),
        TokenKind::Comment => Some(palette.comment),
        TokenKind::Global => Some(palette.external),
        TokenKind::Type | TokenKind::Special => Some(palette.fg_3),
        TokenKind::Plain | TokenKind::Variable | TokenKind::Parameter | TokenKind::Error => None,
    }
}

// A function's C as lines, without the blank lines Ghidra pads it with, or why there is none.
fn code(decompilation: Option<&Decompilation>) -> Result<Vec<Line>, SharedString> {
    let Some(decompilation) = decompilation else {
        return Err("No decompilation.".into());
    };
    let blank = |line: &reve_model::Line| line.tokens.iter().all(|t| t.text.trim().is_empty());
    let lines = &decompilation.lines;
    let start = lines
        .iter()
        .position(|line| !blank(line))
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !blank(line))
        .map_or(start, |end| end + 1);
    let lines = &lines[start..end];
    let diagnostic = |fallback: &str| {
        let message = decompilation.error_message.as_deref().unwrap_or(fallback);
        message.to_owned().into()
    };
    match decompilation.status {
        DecompilationStatus::Succeeded if !lines.is_empty() => {
            Ok(lines.iter().map(Line::new).collect())
        }
        DecompilationStatus::Failed => Err(diagnostic("Ghidra couldn't decompile this function.")),
        DecompilationStatus::TimedOut => Err(diagnostic("Decompilation timed out.")),
        DecompilationStatus::NotAttempted => Err("Not decompiled.".into()),
        _ => Err("No decompilation.".into()),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;
    use reve_model::{Address, Decompilation, DecompilationStatus, Token, TokenKind};

    use super::{Line, code};

    fn line(tokens: &[(&str, TokenKind)]) -> reve_model::Line {
        reve_model::Line {
            tokens: tokens
                .iter()
                .map(|&(text, kind)| Token {
                    text: text.into(),
                    kind,
                })
                .collect(),
            addresses: Vec::new(),
        }
    }

    fn decompilation(
        status: DecompilationStatus,
        lines: &[&str],
        error_message: Option<&str>,
    ) -> Decompilation {
        Decompilation {
            status,
            lines: lines
                .iter()
                .map(|text| line(&[(text, TokenKind::Plain)]))
                .collect(),
            error_message: error_message.map(Into::into),
            ..Decompilation::default()
        }
    }

    #[test]
    fn statuses_map_to_code_or_reason() {
        let mapped = |status, lines: &[&str], error_message| {
            code(Some(&decompilation(status, lines, error_message)))
                .map(|lines| lines.into_iter().map(|line| line.text).collect::<Vec<_>>())
        };
        let reason = |text: &str| Err(SharedString::from(text.to_owned()));
        assert_eq!(
            mapped(
                DecompilationStatus::Succeeded,
                &["", "", "int f(void)", "{", "", "}", " ", ""],
                None
            ),
            Ok(vec![
                "int f(void)".into(),
                "{".into(),
                "".into(),
                "}".into()
            ]),
            "only the padding around the code goes",
        );
        assert_eq!(
            mapped(DecompilationStatus::Failed, &[], Some("Low-level Error")),
            reason("Low-level Error"),
        );
        assert_eq!(
            mapped(DecompilationStatus::TimedOut, &[], None),
            reason("Decompilation timed out."),
        );
        assert_eq!(code(None).map(|_| vec![]), reason("No decompilation."));
        assert_eq!(
            mapped(DecompilationStatus::Succeeded, &[" ", "", " "], None),
            reason("No decompilation."),
            "blank code",
        );
    }

    #[test]
    fn lines_hold_the_instructions_they_came_from() {
        let mut source = line(&[("x = 1;", TokenKind::Plain)]);
        source.addresses = vec![Address(0x1004), Address(0x1008)];
        let mapped = Line::new(&source);
        assert!(mapped.holds(Address(0x1004)) && mapped.holds(Address(0x1008)));
        assert!(!mapped.holds(Address(0x1006)));

        assert_eq!(mapped.target(Address(0x1000)), Some(Address(0x1004)));
        assert_eq!(mapped.target(Address(0x1008)), None, "the cursor is on it");
        let declaration = Line::new(&line(&[("int x;", TokenKind::Plain)]));
        assert_eq!(declaration.target(Address(0x1000)), None, "no code");
    }

    #[test]
    fn spans_cover_each_token() {
        let line = Line::new(&line(&[
            ("  ", TokenKind::Plain),
            ("return", TokenKind::Keyword),
            (" ", TokenKind::Plain),
            ("x", TokenKind::Parameter),
        ]));
        assert_eq!(&*line.text, "  return x");
        assert_eq!(
            line.spans,
            [
                (0..2, TokenKind::Plain),
                (2..8, TokenKind::Keyword),
                (8..9, TokenKind::Plain),
                (9..10, TokenKind::Parameter),
            ]
        );
    }
}
