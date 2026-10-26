use std::ops::Range;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::*;
use reve_model::{Address, Decompilation, DecompilationStatus, TokenKind};

use super::listing::{GUTTER, Listing, ROW};
use super::note;
use crate::palette::Palette;

// The function the listing cursor is in, as Ghidra decompiled it.
pub struct Decompile {
    listing: Entity<Listing>,
    // The shown function's entry.
    entry: Address,
    name: SharedString,
    // Its C, one line each, or why there is none.
    code: Result<Vec<Line>, SharedString>,
    scroll: UniformListScrollHandle,
    _sync: Subscription,
}

// A line of C and the syntax class of each of its spans.
#[derive(Debug, PartialEq)]
struct Line {
    text: SharedString,
    spans: Vec<(Range<usize>, TokenKind)>,
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
        }
    }
}

impl Decompile {
    // Opens on the listing cursor's function and follows the cursor from there.
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let sync = cx.observe(&listing, |this, _, cx| this.sync(cx));
        let function = listing.read(cx).function();
        Self {
            entry: function.entry,
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
    // one shown.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let function = self.listing.read(cx).function();
        if function.entry == self.entry {
            return;
        }
        self.entry = function.entry;
        self.name = function.name.clone().into();
        self.code = code(function.decompilation.as_ref());
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(Point::default());
        cx.notify();
    }
}

impl Render for Decompile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = match &self.code {
            Ok(lines) => lines,
            Err(why) => return note(why.clone(), cx).into_any_element(),
        };
        // The list is as wide as its longest line, so it scrolls sideways to that line's end.
        let longest = (0..lines.len()).max_by_key(|&ix| lines[ix].text.len());

        div()
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
                    lines.len(),
                    cx.processor(|this, range: Range<usize>, _, cx| {
                        let palette = Palette::of(cx);
                        let lines = this.code.as_deref().unwrap_or_default();
                        lines[range]
                            .iter()
                            .map(|line| {
                                let spans = line.spans.iter().filter_map(|(span, kind)| {
                                    Some((span.clone(), hue(*kind, &palette)?.into()))
                                });
                                let text =
                                    StyledText::new(line.text.clone()).with_highlights(spans);
                                div().h(ROW).px(GUTTER).child(text).into_any_element()
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
    use reve_model::{Decompilation, DecompilationStatus, Token, TokenKind};

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
