use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Program};

use super::listing::Listing;
use super::tree_row;
use crate::palette::Palette;

// The tree as it opens: functions shown, the one the listing cursor is in marked. The mark
// follows the cursor from there.
pub fn state(listing: &Entity<Listing>, cx: &mut Context<TreeState>) -> TreeState {
    cx.observe(listing, |tree, listing, cx| {
        mark(tree, listing.read(cx).function().entry, cx)
    })
    .detach();
    let listing = listing.read(cx);
    let (items, entry) = (items(listing.program()), listing.function().entry);
    let mut tree = TreeState::new(cx).items(items);
    mark(&mut tree, entry, cx);
    tree
}

// Selects the function at `entry` and scrolls it into view, unless it already is.
fn mark(tree: &mut TreeState, entry: Address, cx: &mut Context<TreeState>) {
    if let Some(ix) = tree.index_of(&id(entry).into())
        && tree.selected_index() != Some(ix)
    {
        tree.set_selected_index(Some(ix), cx);
        tree.scroll_to_item(ix, ScrollStrategy::Nearest);
    }
}

// The program's functions by name. Function ids are addresses, which outlive renames.
fn items(program: &Program) -> Vec<TreeItem> {
    let mut functions: Vec<_> = program.functions.iter().collect();
    functions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let functions = functions
        .into_iter()
        .map(|function| TreeItem::new(id(function.entry), function.name.clone()));
    vec![
        TreeItem::new("Functions/", "Functions")
            .expanded(true)
            .children(functions),
    ]
}

// A function's id: its entry address.
fn id(entry: Address) -> String {
    format!("{:08x}", entry.0)
}

// Functions in the symbol hue with their address. Ghidra calls the ones nobody has named yet
// `FUN_<address>`; those stay muted, so the color shows analysis progress. Pressing one takes
// every pane to it.
pub fn row(
    listing: Entity<Listing>,
) -> impl Fn(usize, &TreeEntry, bool, &mut Window, &mut App) -> ListItem + 'static {
    move |ix, entry, selected, _, cx| {
        let theme = cx.theme();
        let palette = Palette::of(cx);
        let item = entry.item();
        if entry.is_folder() {
            let hue = theme.muted_foreground;
            return tree_row(ix, entry, "folder-simple-duotone", hue, None, cx);
        }
        let hue = if item.label.starts_with("FUN_") {
            palette.fg_3
        } else {
            palette.symbol
        };
        let address = item.id.trim_start_matches('0').to_owned();
        let target = u64::from_str_radix(&item.id, 16).ok().map(Address);
        let listing = listing.clone();
        tree_row(ix, entry, "function-duotone", hue, Some(address.into()), cx)
            .selected(selected)
            .when_some(target, |row, entry| {
                row.on_click(move |_, _, cx| {
                    listing.update(cx, |listing, cx| listing.go_to(entry, cx))
                })
            })
    }
}
