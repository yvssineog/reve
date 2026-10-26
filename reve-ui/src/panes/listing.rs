use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::base::actions::{Confirm, SelectDown, SelectPageDown, SelectPageUp, SelectUp};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Function, Instruction, Program};

use crate::mock;
use crate::palette::Palette;

// The key context both code panes, listing and decompiler, bind their shortcuts under.
pub const CONTEXT: &str = "Code";
// Instructions a page key moves the cursor.
pub const PAGE: isize = 24;
// The height of every row.
pub const ROW: Pixels = px(18.);
// Column widths, in 12px monospace characters: address 10, bytes 13, mnemonic 7,
// operands 30. Plates, signatures and labels start on the mnemonic column.
const ADDRESS: Pixels = px(72.);
const BYTES: Pixels = px(94.);
const MNEMONIC: Pixels = px(51.);
const OPERANDS: Pixels = px(217.);
pub const GUTTER: Pixels = px(12.);

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

// One 18px row, as the listing shows it. Functions and instructions are indices into the
// program.
enum Row {
    // The hairline between two functions.
    Gap,
    Plate(&'static str),
    Signature(&'static str),
    // A function's name and its XREF summary.
    Label(usize, Option<SharedString>),
    // A function's instruction.
    Code(usize, usize),
}

// What an instruction's operands refer to, which picks their color.
enum Operands<'a> {
    Plain,
    // One of the program's functions, named by its entry address.
    Call(&'a Function),
    // A library's function, which Ghidra writes `<EXTERNAL>::name`.
    External(&'a str),
}

impl<'a> Operands<'a> {
    fn of(program: &'a Program, instruction: &'a Instruction) -> Self {
        if let Some(name) = instruction.operands.strip_prefix("<EXTERNAL>::") {
            return Operands::External(name);
        }
        let function = target(&instruction.operands).and_then(|address| {
            let ix = program
                .functions
                .binary_search_by_key(&address, |function| function.entry)
                .ok()?;
            Some(&program.functions[ix])
        });
        function.map_or(Operands::Plain, Operands::Call)
    }

    // The function called, here or in a library.
    fn call(&self) -> Option<&'a str> {
        match *self {
            Operands::Plain => None,
            Operands::Call(function) => Some(&function.name),
            Operands::External(name) => Some(name),
        }
    }
}

// The address an operand names, in Ghidra's `0x` form.
pub fn target(operands: &str) -> Option<Address> {
    let hex = operands.strip_prefix("0x")?;
    u64::from_str_radix(hex, 16).ok().map(Address)
}

// The end of an instruction's line.
#[derive(Clone, Copy)]
pub enum Eol {
    String(&'static str),
    Comment(&'static str),
}

// Where the cursor is, as the decompiler reads it.
pub struct Spot<'a> {
    pub address: Address,
    pub function: &'a str,
    // The cursor is on the function's entry.
    pub entry: bool,
    pub call: Option<&'a str>,
}

// The program's disassembly, with a cursor on one instruction.
pub struct Listing {
    program: Program,
    rows: Vec<Row>,
    // Each instruction's address and row, in order. Both rise together, so either one
    // finds an instruction by binary search.
    instructions: Vec<(Address, usize)>,
    // A row index, always one of `instructions`.
    cursor: usize,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl Listing {
    // Opens with the cursor on the start function's entry, mid-pane.
    pub fn new(program: Program, cx: &mut Context<Self>) -> Self {
        let rows = layout(&program);
        let instructions = index(&program, &rows);
        let cursor = row_at(&instructions, mock::START).unwrap_or_default();
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(cursor, ScrollStrategy::Center);
        Self {
            program,
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
    pub fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        self.jump(self.moved(by), ScrollStrategy::Nearest, cx);
    }

    // Vim's ctrl-d and ctrl-u: the view and the cursor move half a screen together, so the
    // cursor keeps its place on screen.
    pub fn half_page(&mut self, direction: isize, cx: &mut Context<Self>) {
        let view = self.scroll.0.borrow().base_handle.clone();
        let half = (view.bounds().size.height / ROW / 2.).max(1.) as isize;
        let to = self.moved(direction * half);
        let mut offset = view.offset();
        offset.y -= ROW * (to as f32 - self.cursor as f32);
        view.set_offset(offset);
        self.jump(to, ScrollStrategy::Nearest, cx);
    }

    // Follows the call under the cursor to the function's entry.
    pub fn follow(&mut self, cx: &mut Context<Self>) {
        let (_, instruction) = self.under_cursor();
        if let Operands::Call(function) = Operands::of(&self.program, instruction)
            && let Some(ix) = row_at(&self.instructions, function.entry)
        {
            self.jump(ix, ScrollStrategy::Center, cx);
        }
    }

    pub fn spot(&self) -> Spot<'_> {
        let (function, instruction) = self.under_cursor();
        spot(&self.program, function, instruction)
    }

    // Puts the cursor on the first instruction in its function that calls `name`.
    pub fn pick_call(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let (function, _) = self.under_cursor();
        let Some(ix) = function
            .instructions
            .iter()
            .find(|instruction| Operands::of(&self.program, instruction).call() == Some(name))
            .and_then(|instruction| row_at(&self.instructions, instruction.address))
        else {
            return false;
        };
        self.jump(ix, ScrollStrategy::Nearest, cx);
        true
    }

    // The instruction under the cursor, and its function.
    fn under_cursor(&self) -> (&Function, &Instruction) {
        let Row::Code(function, instruction) = self.rows[self.cursor] else {
            unreachable!("the cursor is always on an instruction");
        };
        let function = &self.program.functions[function];
        (function, &function.instructions[instruction])
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
            Row::Label(function, xrefs) => header(h_flex().w_full().h(ROW).px(GUTTER))
                .gap_4()
                .child(
                    div()
                        .flex_none()
                        .font_semibold()
                        .text_color(palette.symbol)
                        .child(self.program.functions[*function].name.clone()),
                )
                .children(xrefs.clone().map(|xrefs| {
                    div()
                        .ml_auto()
                        .min_w_0()
                        .truncate()
                        .text_color(palette.fg_4)
                        .child(xrefs)
                }))
                .into_any_element(),
            Row::Code(function, instruction) => {
                let instruction = &self.program.functions[*function].instructions[*instruction];
                self.instruction(ix, instruction, palette, cx)
            }
        }
    }

    // Pressing puts the cursor on the row; a double click follows a call.
    fn instruction(
        &self,
        ix: usize,
        instruction: &Instruction,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (operands, hue) = match Operands::of(&self.program, instruction) {
            Operands::Plain => (&instruction.operands, palette.fg),
            Operands::Call(function) => (&function.name, palette.symbol),
            Operands::External(_) => (&instruction.operands, palette.external),
        };
        let eol = mock::eol(instruction.address).map(|eol| match eol {
            Eol::String(text) => (format!("= \"{text}\""), palette.string),
            Eol::Comment(text) => (format!("; {text}"), palette.comment),
        });
        let bytes: Vec<String> = instruction
            .bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();

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
            .child(cell(
                ADDRESS,
                format!("{:08x}", instruction.address.0),
                palette.fg_3,
            ))
            .child(cell(BYTES, bytes.join(" "), palette.fg_4))
            .child(cell(
                MNEMONIC,
                instruction.mnemonic.clone(),
                palette.mnemonic,
            ))
            .child(cell(OPERANDS, operands.clone(), hue))
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

// Each function's plate, signature and label over its instructions, a gap between functions.
fn layout(program: &Program) -> Vec<Row> {
    let mut xrefs = xrefs(program);
    let mut rows = Vec::new();
    for (ix, function) in program.functions.iter().enumerate() {
        if ix > 0 {
            rows.push(Row::Gap);
        }
        if let Some(&(_, plate, code)) = mock::DECOMPILED
            .iter()
            .find(|&&(name, ..)| name == function.name)
        {
            rows.extend(plate.map(Row::Plate));
            rows.extend(code.lines().next().map(Row::Signature));
        }
        rows.push(Row::Label(ix, xrefs.remove(&function.entry)));
        rows.extend((0..function.instructions.len()).map(|instruction| Row::Code(ix, instruction)));
    }
    rows
}

// Who calls each function, by its entry: `XREF[n]: caller:address(c), ...`, in address order.
fn xrefs(program: &Program) -> HashMap<Address, SharedString> {
    let mut callers: HashMap<Address, Vec<String>> = HashMap::new();
    for function in &program.functions {
        for instruction in &function.instructions {
            if let Operands::Call(callee) = Operands::of(program, instruction) {
                let caller = format!("{}:{:08x}(c)", function.name, instruction.address.0);
                callers.entry(callee.entry).or_default().push(caller);
            }
        }
    }
    callers
        .into_iter()
        .map(|(entry, callers)| {
            let xrefs = format!("XREF[{}]: {}", callers.len(), callers.join(", "));
            (entry, xrefs.into())
        })
        .collect()
}

// Each instruction's address and row, in row order.
fn index(program: &Program, rows: &[Row]) -> Vec<(Address, usize)> {
    rows.iter()
        .enumerate()
        .filter_map(|(ix, row)| match *row {
            Row::Code(function, instruction) => Some((
                program.functions[function].instructions[instruction].address,
                ix,
            )),
            _ => None,
        })
        .collect()
}

// The row of the instruction at `address`.
fn row_at(instructions: &[(Address, usize)], address: Address) -> Option<usize> {
    let at = instructions
        .binary_search_by_key(&address, |&(address, _)| address)
        .ok()?;
    Some(instructions[at].1)
}

// Where `instruction` sits in `function`.
pub(super) fn spot<'a>(
    program: &'a Program,
    function: &'a Function,
    instruction: &'a Instruction,
) -> Spot<'a> {
    Spot {
        address: instruction.address,
        function: &function.name,
        entry: instruction.address == function.entry,
        call: Operands::of(program, instruction).call(),
    }
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
    use reve_model::Address;

    use super::{Operands, index, layout, row_at, xrefs};
    use crate::mock;

    #[test]
    fn calls_land_on_instructions() {
        let program = mock::stress(mock::program());
        let rows = layout(&program);
        let instructions = index(&program, &rows);
        assert!(rows.len() > 1_000_000);
        assert!(
            instructions.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "addresses rise row by row"
        );
        assert!(row_at(&instructions, mock::START).is_some());
        for function in &program.functions {
            for instruction in &function.instructions {
                if instruction.mnemonic == "bl" {
                    assert!(
                        Operands::of(&program, instruction).call().is_some(),
                        "{} at {:08x}",
                        instruction.operands,
                        instruction.address.0
                    );
                }
            }
        }
    }

    #[test]
    fn xrefs_name_callers() {
        let xrefs = xrefs(&mock::program());
        assert_eq!(
            &*xrefs[&Address(0x00413480)],
            "XREF[3]: handle_client:00411f50(c), parse_request:00412a68(c), serve_file:00414444(c)"
        );
        assert!(
            !xrefs.contains_key(&Address(0x00410c40)),
            "nothing calls main"
        );
    }
}
