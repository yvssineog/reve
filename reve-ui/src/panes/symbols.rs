use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::*;
use reve_model::{Address, Program};

use super::tree_row;
use crate::palette::Palette;

// The tree as it opens: functions shown.
pub fn state(program: &Program, cx: &mut Context<TreeState>) -> TreeState {
    TreeState::new(cx).items(items(program))
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
// `FUN_<address>`; those stay muted, so the color shows analysis progress.
pub fn row(ix: usize, entry: &TreeEntry, _: bool, _: &mut Window, cx: &mut App) -> ListItem {
    let theme = cx.theme();
    let palette = Palette::of(cx);
    let item = entry.item();
    let (icon, hue, address) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground, None)
    } else {
        let hue = if item.label.starts_with("FUN_") {
            palette.fg_3
        } else {
            palette.symbol
        };
        let address = item.id.trim_start_matches('0').to_owned();
        ("function-duotone", hue, Some(address.into()))
    };
    tree_row(ix, entry, icon, hue, address, cx)
}
