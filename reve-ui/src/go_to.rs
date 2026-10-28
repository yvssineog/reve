use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use reve_model::{Address, Function};

use crate::icon;
use crate::menu::{self, Note, Pick, Row, Section};
use crate::palette::Palette;
use crate::panes::listing::Listing;

// Places Go To lists at most.
const SHOWN: usize = 8;

// Opens Go To below the header: an address or a function's name, and the places it could
// mean. Picking one jumps the listing there and hands it the keyboard.
pub fn open(listing: Entity<Listing>, window: &mut Window, cx: &mut App) {
    let font = cx.theme().mono_font_family.clone();
    menu::open(
        "Address or symbol",
        "No address or function by that name",
        font,
        move |query, cx| places(&listing, query, cx),
        window,
        cx,
    );
}

// What `query` could mean, best first: the instruction at the address it spells or the next
// one after, then the functions it names. Each wears its pane's glyph.
fn places(listing: &Entity<Listing>, query: &str, cx: &App) -> Vec<Section> {
    let palette = Palette::of(cx);
    let view = listing.read(cx);
    let at: Vec<Row> = address(query)
        .and_then(|address| view.at_or_after(address))
        .map(|(function, instruction)| Row {
            icon: icon("cpu-duotone", menu::GLYPH).text_color(palette.mnemonic),
            label: view.hex(instruction.address).into(),
            note: Note::Text(function.name.clone().into()),
            pick: jump(listing, instruction.address),
        })
        .into_iter()
        .collect();
    let named = named(&view.program().functions, query)
        .into_iter()
        .take(SHOWN - at.len())
        .map(|function| Row {
            icon: icon("function-duotone", menu::GLYPH).text_color(palette.name(&function.name)),
            label: function.name.clone().into(),
            note: Note::Text(view.hex(function.entry).into()),
            pick: jump(listing, function.entry),
        })
        .collect();
    vec![
        Section {
            heading: "Address",
            rows: at,
        },
        Section {
            heading: "Functions",
            rows: named,
        },
    ]
}

// Jumps the listing to `address` and hands it the keyboard.
fn jump(listing: &Entity<Listing>, address: Address) -> Pick {
    let listing = listing.clone();
    Rc::new(move |window, cx| {
        listing.update(cx, |listing, cx| listing.go_to(address, cx));
        window.focus(&listing.focus_handle(cx), cx);
    })
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
