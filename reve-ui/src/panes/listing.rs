use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::base::actions::{Confirm, SelectDown, SelectPageDown, SelectPageUp, SelectUp};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Function, Instruction, Program};

use crate::palette::Palette;

// The key context the listing binds its shortcuts under.
const CONTEXT: &str = "Code";
// Instructions a page key moves the cursor.
const PAGE: isize = 24;
// The height of every row.
pub const ROW: Pixels = px(18.);
// One 12px monospace character.
const CHAR: Pixels = px(7.2);
// Column widths, in characters: address as long as the program's addresses plus 2, bytes 13,
// mnemonic 7, operands 30. Labels start on the mnemonic column.
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
    // A function's name and its XREF summary.
    Label(usize, Option<SharedString>),
    // A function's instruction.
    Code(usize, usize),
}

// The function `instruction` calls: the one whose entry its operands name.
fn callee<'a>(program: &'a Program, instruction: &Instruction) -> Option<&'a Function> {
    let address = target(&instruction.operands)?;
    let ix = program
        .functions
        .binary_search_by_key(&address, |function| function.entry)
        .ok()?;
    Some(&program.functions[ix])
}

// The address an operand names, in Ghidra's `0x` form.
fn target(operands: &str) -> Option<Address> {
    let hex = operands.strip_prefix("0x")?;
    u64::from_str_radix(hex, 16).ok().map(Address)
}

// The program's disassembly, with a cursor on one instruction.
pub struct Listing {
    program: Program,
    rows: Vec<Row>,
    // Each instruction's row, in row order.
    instructions: Vec<usize>,
    // Each instruction's address and row, in address order. A function's body can be split
    // around other functions, so this order isn't row order.
    addresses: Vec<(Address, usize)>,
    // A row index, always one of `instructions`.
    cursor: usize,
    // Hex digits every address shows with, so the address column fits the longest.
    digits: usize,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl Listing {
    // Opens with the cursor on the first instruction.
    pub fn new(program: Program, cx: &mut Context<Self>) -> Self {
        let digits = digits(&program);
        let rows = layout(&program, digits);
        let (instructions, addresses) = index(&program, &rows);
        let cursor = instructions.first().copied().unwrap_or(0);
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(cursor, ScrollStrategy::Center);
        Self {
            program,
            rows,
            instructions,
            addresses,
            cursor,
            scroll,
            digits,
            focus: cx.focus_handle(),
        }
    }

    // The address column's width.
    pub fn address_width(&self) -> Pixels {
        CHAR * (self.digits + 2) as f32
    }

    // `address` in hex, padded like every address the listing shows.
    pub fn hex(&self, address: Address) -> String {
        hex(address, self.digits)
    }

    // Puts the cursor on row `ix` and scrolls it into view.
    fn jump(&mut self, ix: usize, strategy: ScrollStrategy, cx: &mut Context<Self>) {
        self.cursor = ix;
        self.scroll.scroll_to_item(ix, strategy);
        cx.notify();
    }

    // The row of the instruction `by` instructions from the cursor, held at both ends.
    fn moved(&self, by: isize) -> usize {
        let at = self.instructions.binary_search(&self.cursor).unwrap_or(0);
        let at = at
            .saturating_add_signed(by)
            .min(self.instructions.len() - 1);
        self.instructions[at]
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
        let (_, instruction) = self.under_cursor();
        if let Some(function) = callee(&self.program, instruction) {
            self.go_to(function.entry, ScrollStrategy::Center, cx);
        }
    }

    // Puts the cursor on the instruction at `address`. Every pane follows the cursor, so this
    // is how any of them goes somewhere.
    pub fn go_to(&mut self, address: Address, strategy: ScrollStrategy, cx: &mut Context<Self>) {
        if let Some(ix) = row_at(&self.addresses, address) {
            self.jump(ix, strategy, cx);
        }
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    // The function the cursor is in.
    pub fn function(&self) -> &Function {
        self.under_cursor().0
    }

    // The address of the instruction under the cursor.
    pub fn address(&self) -> Address {
        self.under_cursor().1.address
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
        match &self.rows[ix] {
            Row::Gap => h_flex()
                .h(ROW)
                .w_full()
                .child(div().flex_1().h_px().bg(palette.hairline))
                .into_any_element(),
            Row::Label(function, xrefs) => {
                header(h_flex().w_full().h(ROW).px(GUTTER), self.address_width())
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
                    .into_any_element()
            }
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
        let (operands, hue) = match callee(&self.program, instruction) {
            Some(function) => (&function.name, palette.symbol),
            None => (&instruction.operands, palette.fg),
        };
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
                self.address_width(),
                hex(instruction.address, self.digits),
                palette.fg_3,
            ))
            .child(cell(BYTES, bytes.join(" "), palette.fg_4))
            .child(cell(
                MNEMONIC,
                instruction.mnemonic.clone(),
                palette.mnemonic,
            ))
            .child(cell(OPERANDS, operands.clone(), hue))
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

// Each function's label over its instructions, a gap between functions.
fn layout(program: &Program, digits: usize) -> Vec<Row> {
    let mut xrefs = xrefs(program, digits);
    let mut rows = Vec::new();
    for (ix, function) in program.functions.iter().enumerate() {
        if ix > 0 {
            rows.push(Row::Gap);
        }
        rows.push(Row::Label(ix, xrefs.remove(&function.entry)));
        rows.extend((0..function.instructions.len()).map(|instruction| Row::Code(ix, instruction)));
    }
    rows
}

// Who calls each function, by its entry: `XREF[n]: caller:address(c), ...`, in address order.
fn xrefs(program: &Program, digits: usize) -> HashMap<Address, SharedString> {
    let mut callers: HashMap<Address, Vec<String>> = HashMap::new();
    for function in &program.functions {
        for instruction in &function.instructions {
            if let Some(callee) = callee(program, instruction) {
                let caller = format!("{}:{}(c)", function.name, hex(instruction.address, digits));
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

// Each instruction's row in row order, and its address and row in address order.
fn index(program: &Program, rows: &[Row]) -> (Vec<usize>, Vec<(Address, usize)>) {
    let mut addresses: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(ix, row)| match *row {
            Row::Code(function, instruction) => Some((
                program.functions[function].instructions[instruction].address,
                ix,
            )),
            _ => None,
        })
        .collect();
    let instructions = addresses.iter().map(|&(_, row)| row).collect();
    addresses.sort_unstable();
    (instructions, addresses)
}

// The row of the instruction at `address`.
fn row_at(addresses: &[(Address, usize)], address: Address) -> Option<usize> {
    let at = addresses
        .binary_search_by_key(&address, |&(address, _)| address)
        .ok()?;
    Some(addresses[at].1)
}

// A header row's text, started on the mnemonic column and cut with an ellipsis.
fn header(row: Div, address_width: Pixels) -> Div {
    row.pl(GUTTER + address_width + BYTES).truncate()
}

// Hex digits for the program's highest instruction address, at least 8.
fn digits(program: &Program) -> usize {
    let instructions = program.functions.iter().flat_map(|f| &f.instructions);
    let highest = instructions.map(|i| i.address.0).max().unwrap_or(0);
    format!("{highest:x}").len().max(8)
}

// `address` in hex, zero-padded to `digits`.
fn hex(address: Address, digits: usize) -> String {
    format!("{:0digits$x}", address.0)
}

// A fixed-width column on one line, its text cut with an ellipsis at the edge.
fn cell(width: Pixels, text: impl Into<SharedString>, hue: Hsla) -> Div {
    div()
        .flex_none()
        .w(width)
        .truncate()
        .text_color(hue)
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use reve_model::{Address, Function, Instruction, Program};

    use super::{callee, digits, hex, index, layout, row_at, xrefs};

    fn function(name: &str, entry: u64, code: &[(&str, &str)]) -> Function {
        let instructions: Vec<_> = (0..)
            .zip(code)
            .map(|(ix, &(mnemonic, operands))| Instruction {
                address: Address(entry + 4 * ix),
                size: 4,
                mnemonic: mnemonic.into(),
                operands: operands.into(),
                bytes: vec![0; 4],
            })
            .collect();
        Function {
            name: name.into(),
            entry: Address(entry),
            size: 4 * instructions.len() as u64,
            instructions,
            decompilation: None,
        }
    }

    fn program() -> Program {
        Program {
            name: "demo".into(),
            format: "ELF".into(),
            architecture: "AARCH64:LE:64:v8A".into(),
            address_size_bits: 64,
            functions: vec![
                function(
                    "main",
                    0x1000,
                    &[("bl", "0x1010"), ("bl", "0x1010"), ("ret", "")],
                ),
                function("helper", 0x1010, &[("mov", "x0, #0x1"), ("bl", "0x2000")]),
                function("leaf", 0x1020, &[("ret", "")]),
            ],
        }
    }

    #[test]
    fn calls_resolve_to_function_entries() {
        let program = program();
        let name = |f: usize, i: usize| {
            callee(&program, &program.functions[f].instructions[i]).map(|f| f.name.as_str())
        };
        assert_eq!(name(0, 0), Some("helper"));
        assert_eq!(name(1, 0), None, "operands that aren't one address");
        assert_eq!(name(1, 1), None, "an address no function starts at");
    }

    #[test]
    fn xrefs_name_callers() {
        let xrefs = xrefs(&program(), 8);
        assert_eq!(
            &*xrefs[&Address(0x1010)],
            "XREF[2]: main:00001000(c), main:00001004(c)"
        );
        assert_eq!(xrefs.len(), 1, "nothing calls main or leaf");
    }

    #[test]
    fn split_functions_find_every_instruction() {
        let mut program = program();
        program.functions[0].instructions[2].address = Address(0x1030);
        let rows = layout(&program, 8);
        let (instructions, addresses) = index(&program, &rows);
        for &(address, row) in &addresses {
            assert_eq!(row_at(&addresses, address), Some(row), "{:x}", address.0);
        }
        assert_eq!(instructions, [1, 2, 3, 6, 7, 10], "steps follow rows");
    }

    #[test]
    fn addresses_pad_to_the_longest() {
        let mut program = program();
        assert_eq!(digits(&program), 8, "never fewer than 8");
        program.functions[2].instructions[0].address = Address(0xffff_8000_0000_1020);
        let digits = digits(&program);
        assert_eq!(digits, 16);
        assert_eq!(hex(Address(0x1000), digits), "0000000000001000");
    }
}
