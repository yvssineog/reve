use std::cell::Cell;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Function};

use crate::palette::Palette;
use crate::panes::listing::Listing;

// Places the picker lists at most.
const SHOWN: usize = 8;

// Go To: an address or a function's name, and the places it could mean. Picking one jumps the
// listing there and hands it the keyboard.
pub struct Picker {
    listing: Entity<Listing>,
    query: Entity<CommandState>,
    places: Vec<Place>,
}

// Somewhere the query leads: an instruction by its address, or a function by its name.
struct Place {
    label: SharedString,
    // The instruction's function, or the function's address.
    detail: SharedString,
    address: Address,
}

// Opens Go To below the header, its field focused.
pub fn open(listing: Entity<Listing>, window: &mut Window, cx: &mut App) {
    let query = cx.new(|cx| CommandState::new(window, cx));
    let picker = cx.new(|_| Picker {
        listing,
        query: query.clone(),
        places: Vec::new(),
    });
    // The dialog takes focus as it opens; hand it to the field once, not on every rebuild.
    let opening = Cell::new(true);
    window.open_dialog(cx, move |dialog, window, cx| {
        if opening.replace(false) {
            query.update(cx, |query, cx| query.focus(window, cx));
        }
        dialog
            .w(px(440.))
            .margin_top(px(72.))
            .p_0()
            .min_h_0()
            .close_button(false)
            .entrance(DialogEntrance::None)
            .child(picker.clone())
    });
}

impl Picker {
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.places = places(self.listing.read(cx), query.trim());
        cx.notify();
    }

    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(address) = self.places.get(ix).map(|place| place.address) else {
            return;
        };
        self.listing
            .update(cx, |listing, cx| listing.go_to(address, cx));
        window.close_dialog(cx);
        window.focus(&self.listing.focus_handle(cx), cx);
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (searched, picked) = (cx.entity().downgrade(), cx.entity().downgrade());
        let items = self.places.iter().map(|place| {
            let (label, detail) = (place.label.clone(), place.detail.clone());
            CommandItem::new()
                .label(label.clone())
                .child(move |_, cx| row(label.clone(), detail.clone(), cx))
        });
        Command::new(&self.query)
            .bordered(false)
            .filterable(false)
            .placeholder("Address or symbol")
            .items(items)
            // Nothing typed, nothing to say.
            .empty(|query, _, cx| {
                div().when(!query.query(cx).is_empty(), |empty| {
                    empty
                        .py_4()
                        .text_center()
                        .text_sm()
                        .text_color(Palette::of(cx).fg_3)
                        .child("No address or function by that name")
                })
            })
            .on_query(move |query, _, cx| {
                searched
                    .update(cx, |picker, cx| picker.search(query, cx))
                    .ok();
            })
            .on_confirm(move |ix, window, cx| {
                picked
                    .update(cx, |picker, cx| picker.pick(ix.row, window, cx))
                    .ok();
            })
    }
}

// A place in monospace, what it is quieter at the right.
fn row(label: SharedString, detail: SharedString, cx: &App) -> Div {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_3()
        .font_family(cx.theme().mono_font_family.clone())
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(Palette::of(cx).fg_3)
                .child(detail),
        )
}

// What `query` could mean, best first: the instruction at the address it spells or the next
// one after, then the functions it names.
fn places(listing: &Listing, query: &str) -> Vec<Place> {
    let at = address(query).and_then(|address| listing.at_or_after(address));
    let at = at.map(|(function, instruction)| Place {
        label: listing.hex(instruction.address).into(),
        detail: function.name.clone().into(),
        address: instruction.address,
    });
    let named = named(&listing.program().functions, query)
        .into_iter()
        .map(|function| Place {
            label: function.name.clone().into(),
            detail: listing.hex(function.entry).into(),
            address: function.entry,
        });
    at.into_iter().chain(named).take(SHOWN).collect()
}

// The address `query` spells in hex, with or without `0x`.
fn address(query: &str) -> Option<Address> {
    let digits = query.strip_prefix("0x").unwrap_or(query);
    u64::from_str_radix(digits, 16).ok().map(Address)
}

// Functions with code whose names hold `query`, ignoring case: the one it names first, then
// those starting with it, then the rest, each in address order.
fn named<'a>(functions: &'a [Function], query: &str) -> Vec<&'a Function> {
    if query.is_empty() {
        return Vec::new();
    }
    let query = query.to_lowercase();
    let mut named: Vec<_> = functions
        .iter()
        .filter(|function| !function.instructions.is_empty())
        .filter_map(|function| {
            let name = function.name.to_lowercase();
            let rank = if name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else if name.contains(&query) {
                2
            } else {
                return None;
            };
            Some((rank, function))
        })
        .collect();
    named.sort_by_key(|&(rank, _)| rank);
    named.into_iter().map(|(_, function)| function).collect()
}

#[cfg(test)]
mod tests {
    use reve_model::{Address, Function, Instruction};

    use super::{address, named};

    fn function(name: &str, entry: u64, code: bool) -> Function {
        let instruction = Instruction {
            address: Address(entry),
            size: 4,
            mnemonic: "ret".into(),
            operands: String::new(),
            bytes: vec![0; 4],
        };
        Function {
            name: name.into(),
            entry: Address(entry),
            size: 4,
            instructions: if code { vec![instruction] } else { Vec::new() },
            decompilation: None,
        }
    }

    #[test]
    fn names_rank_whole_then_start_then_inside() {
        let functions = [
            function("domain", 0x1000, true),
            function("main_loop", 0x1010, true),
            function("main", 0x1020, true),
            function("FUN_00001030", 0x1030, true),
            function("mainframe", 0x2000, false),
        ];
        let names = |query| -> Vec<_> {
            named(&functions, query)
                .into_iter()
                .map(|function| function.name.as_str())
                .collect()
        };
        assert_eq!(
            names("MAIN"),
            ["main", "main_loop", "domain"],
            "functions without code have nowhere to go"
        );
        assert_eq!(names("fun_"), ["FUN_00001030"]);
        assert!(names("").is_empty());
    }

    #[test]
    fn addresses_read_as_hex() {
        assert_eq!(address("0x401000"), Some(Address(0x401000)));
        assert_eq!(address("401a"), Some(Address(0x401a)));
        assert_eq!(address("main"), None);
        assert_eq!(address(""), None);
    }
}
