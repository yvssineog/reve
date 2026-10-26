mod mock;

use std::ops::Range;

use gpui_kit::base::actions::{Confirm, SelectDown, SelectPageDown, SelectPageUp, SelectUp};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::palette::Palette;
use mock::START;

// The key context the listing's shortcuts are bound under.
const CONTEXT: &str = "Listing";
// Instructions a page key moves the cursor.
const PAGE: isize = 24;
// The height of every row.
const ROW: Pixels = px(18.);
// Column widths, in 12px monospace characters: address 10, bytes 13, mnemonic 7,
// operands 30. Plates, signatures and labels start on the mnemonic column.
const ADDRESS: Pixels = px(72.);
const BYTES: Pixels = px(94.);
const MNEMONIC: Pixels = px(51.);
const OPERANDS: Pixels = px(217.);
const GUTTER: Pixels = px(12.);

actions!(listing, [HalfPageDown, HalfPageUp]);

// Arrows or j/k step one instruction, page keys a page, ctrl-d and ctrl-u half a screen,
// enter follows a call.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("k", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("j", SelectDown, Some(CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(CONTEXT)),
        KeyBinding::new("ctrl-d", HalfPageDown, Some(CONTEXT)),
        KeyBinding::new("ctrl-u", HalfPageUp, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
    ]);
}

// One 18px row, as the listing shows it.
#[derive(Clone, Copy)]
pub enum Row {
    // The hairline between two functions.
    Gap,
    Plate(&'static str),
    Signature(&'static str),
    // A function's name and its XREF summary.
    Label(&'static str, Option<&'static str>),
    Code(Code),
}

// An instruction, then what it loads or its comment.
#[derive(Clone, Copy)]
pub struct Code {
    address: u32,
    bytes: &'static str,
    mnemonic: &'static str,
    operands: Operands,
    eol: Option<Eol>,
}

// Operands by what they refer to, which picks their color.
#[derive(Clone, Copy)]
pub enum Operands {
    Plain(&'static str),
    // A function here, by name and entry address.
    Call(&'static str, u32),
    External(&'static str),
}

// The end of an instruction's line.
#[derive(Clone, Copy)]
pub enum Eol {
    String(&'static str),
    Comment(&'static str),
}

// The program's disassembly, with a cursor on one instruction.
pub struct Listing {
    rows: Vec<Row>,
    // Each instruction's address and row, in order. Both rise together, so either one
    // finds an instruction by binary search.
    instructions: Vec<(u32, usize)>,
    // A row index, always one of `instructions`.
    cursor: usize,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl Listing {
    // Opens with the cursor on the start function's entry, mid-pane.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let rows = mock::rows();
        let instructions = index(&rows);
        let cursor = entry(&instructions, START).unwrap_or_default();
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(cursor, ScrollStrategy::Center);
        Self {
            rows,
            instructions,
            cursor,
            scroll,
            focus: cx.focus_handle(),
        }
    }

    // Puts the cursor on row `ix` and scrolls it into view.
    fn jump(&mut self, ix: usize, strategy: ScrollStrategy, cx: &mut Context<Self>) {
        self.cursor = ix;
        self.scroll.scroll_to_item(ix, strategy);
        cx.notify();
    }

    // The row of the instruction `by` instructions from the cursor, held at both ends.
    fn moved(&self, by: isize) -> usize {
        let at = self
            .instructions
            .binary_search_by_key(&self.cursor, |&(_, row)| row)
            .unwrap_or(0);
        let at = at
            .saturating_add_signed(by)
            .min(self.instructions.len() - 1);
        self.instructions[at].1
    }

    // Moves the cursor `by` instructions, scrolling no more than needed.
    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        self.jump(self.moved(by), ScrollStrategy::Nearest, cx);
    }

    // Vim's ctrl-d and ctrl-u: the view and the cursor move half a screen together, so the
    // cursor keeps its place on screen.
    fn half_page(&mut self, direction: isize, cx: &mut Context<Self>) {
        let view = self.scroll.0.borrow().base_handle.clone();
        let half = (view.bounds().size.height / ROW / 2.).max(1.) as isize;
        let to = self.moved(direction * half);
        let mut offset = view.offset();
        offset.y -= ROW * (to as f32 - self.cursor as f32);
        view.set_offset(offset);
        self.jump(to, ScrollStrategy::Nearest, cx);
    }

    // Follows the call under the cursor to the function's entry.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if let Row::Code(Code {
            operands: Operands::Call(_, address),
            ..
        }) = self.rows[self.cursor]
            && let Some(ix) = entry(&self.instructions, address)
        {
            self.jump(ix, ScrollStrategy::Center, cx);
        }
    }

    // Row `ix`, inset by the gutter, which will hold bookmarks.
    fn row(&self, ix: usize, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let line = div().h(ROW).px(GUTTER);
        match &self.rows[ix] {
            Row::Gap => h_flex()
                .h(ROW)
                .w_full()
                .child(div().flex_1().h_px().bg(palette.hairline))
                .into_any_element(),
            Row::Plate(note) => header(line)
                .text_color(palette.fg_3)
                .child(format!("/* {note} */"))
                .into_any_element(),
            Row::Signature(signature) => header(line)
                .text_color(palette.fg_3)
                .child(*signature)
                .into_any_element(),
            Row::Label(name, xrefs) => header(h_flex().w_full().h(ROW).px(GUTTER))
                .gap_4()
                .child(
                    div()
                        .flex_none()
                        .font_semibold()
                        .text_color(palette.symbol)
                        .child(*name),
                )
                .children(xrefs.map(|xrefs| {
                    div()
                        .ml_auto()
                        .min_w_0()
                        .truncate()
                        .text_color(palette.fg_4)
                        .child(xrefs)
                }))
                .into_any_element(),
            Row::Code(code) => self.code(ix, code, palette, cx),
        }
    }

    // Pressing puts the cursor on the row; a double click follows a call.
    fn code(
        &self,
        ix: usize,
        code: &Code,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (operands, hue): (SharedString, _) = match code.operands {
            Operands::Plain(text) => (text.into(), palette.fg),
            Operands::Call(name, _) => (name.into(), palette.symbol),
            Operands::External(name) => (format!("<EXTERNAL>::{name}").into(), palette.external),
        };
        let eol = code.eol.as_ref().map(|eol| match eol {
            Eol::String(text) => (format!("= \"{text}\""), palette.string),
            Eol::Comment(text) => (format!("; {text}"), palette.comment),
        });

        h_flex()
            .id(ix)
            .w_full()
            .h(ROW)
            .px(GUTTER)
            .map(|row| {
                if ix == self.cursor {
                    row.bg(palette.cursor)
                } else {
                    row.hover(|row| row.bg(palette.fg.opacity(0.04)))
                }
            })
            .child(cell(ADDRESS, format!("{:08x}", code.address), palette.fg_3))
            .child(cell(BYTES, code.bytes, palette.fg_4))
            .child(cell(MNEMONIC, code.mnemonic, palette.mnemonic))
            .child(cell(OPERANDS, operands, hue))
            .children(eol.map(|(text, hue)| div().min_w_0().truncate().text_color(hue).child(text)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.jump(ix, ScrollStrategy::Nearest, cx)),
            )
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                if event.click_count() == 2 {
                    this.follow(cx);
                }
            }))
            .into_any_element()
    }
}

impl Focusable for Listing {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Listing {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            // A press anywhere in the listing focuses it.
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.step(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &SelectPageUp, _, cx| this.step(-PAGE, cx)))
            .on_action(cx.listener(|this, _: &SelectPageDown, _, cx| this.step(PAGE, cx)))
            .on_action(cx.listener(|this, _: &HalfPageDown, _, cx| this.half_page(1, cx)))
            .on_action(cx.listener(|this, _: &HalfPageUp, _, cx| this.half_page(-1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.follow(cx)))
            .flex_1()
            .min_h_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.))
            .line_height(px(18.))
            .child(
                uniform_list(
                    "listing",
                    self.rows.len(),
                    cx.processor(|this, range: Range<usize>, _, cx| {
                        let palette = Palette::of(cx);
                        range.map(|ix| this.row(ix, &palette, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .size_full()
                .pt_1()
                .pb_10(),
            )
            .vertical_scrollbar(&self.scroll)
    }
}

// Each instruction's address and row, in row order.
fn index(rows: &[Row]) -> Vec<(u32, usize)> {
    rows.iter()
        .enumerate()
        .filter_map(|(ix, row)| match row {
            Row::Code(code) => Some((code.address, ix)),
            _ => None,
        })
        .collect()
}

// The row of the instruction at `address`.
fn entry(instructions: &[(u32, usize)], address: u32) -> Option<usize> {
    let at = instructions
        .binary_search_by_key(&address, |&(address, _)| address)
        .ok()?;
    Some(instructions[at].1)
}

// A header row's text, started on the mnemonic column and cut with an ellipsis.
fn header(row: Div) -> Div {
    row.pl(GUTTER + ADDRESS + BYTES).truncate()
}

// A fixed-width column, its text cut at the edge.
fn cell(width: Pixels, text: impl Into<SharedString>, hue: Hsla) -> Div {
    div()
        .flex_none()
        .w(width)
        .overflow_hidden()
        .text_color(hue)
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::{Operands, Row, entry, index, mock};

    #[test]
    fn calls_land_on_instructions() {
        let rows = mock::rows();
        let instructions = index(&rows);
        assert!(rows.len() > 1_000_000);
        assert!(
            instructions.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "addresses rise row by row"
        );
        assert!(entry(&instructions, mock::START).is_some());
        for row in &rows {
            if let Row::Code(code) = row
                && let Operands::Call(name, address) = code.operands
            {
                assert!(
                    entry(&instructions, address).is_some(),
                    "{name} at {address:08x}"
                );
            }
        }
    }
}
