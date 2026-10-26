use std::collections::BTreeMap;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::*;
use reve_model::{Address, Program};

use super::tree_row;
use crate::mock::{self, IMPORTS, LIBRARY, NAMESPACES};
use crate::palette::Palette;

// The tree as it opens: functions shown by namespace, imports folded, the start function selected.
pub fn state(program: &Program, cx: &mut Context<TreeState>) -> TreeState {
    let mut state = TreeState::new(cx).items(items(program));
    let start = id(mock::START).into();
    state.reveal_item(&start, ScrollStrategy::Nearest, cx);
    state.set_selected_index(state.index_of(&start), cx);
    state
}

// Namespaces then unscoped functions, each by name; imports under their library.
// Function ids are addresses, which outlive renames. Folder ids end in `/`.
fn items(program: &Program) -> Vec<TreeItem> {
    let mut functions: Vec<_> = program.functions.iter().collect();
    functions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let mut namespaces: BTreeMap<&str, Vec<TreeItem>> = BTreeMap::new();
    let mut unscoped = Vec::new();
    for function in functions {
        let leaf = TreeItem::new(id(function.entry), function.name.clone());
        match NAMESPACES.iter().find(|&&(name, _)| name == function.name) {
            Some(&(_, namespace)) => namespaces.entry(namespace).or_default().push(leaf),
            None => unscoped.push(leaf),
        }
    }

    let namespaces = namespaces.into_iter().map(|(namespace, children)| {
        TreeItem::new(format!("Functions/{namespace}/"), namespace)
            .expanded(true)
            .children(children)
    });
    let imports = IMPORTS
        .iter()
        .map(|&name| TreeItem::new(format!("Imports/{LIBRARY}/{name}"), name));
    vec![
        TreeItem::new("Functions/", "Functions")
            .expanded(true)
            .children(namespaces.chain(unscoped)),
        TreeItem::new("Imports/", "Imports")
            .child(TreeItem::new(format!("Imports/{LIBRARY}/"), LIBRARY).children(imports)),
    ]
}

// A function's id: its entry address.
fn id(entry: Address) -> String {
    format!("{:08x}", entry.0)
}

// Functions in the symbol hue with their address. Ghidra calls the ones nobody has named yet
// `FUN_<address>`; those stay muted, so the color shows analysis progress. Imports in the external hue.
pub fn row(ix: usize, entry: &TreeEntry, _: bool, _: &mut Window, cx: &mut App) -> ListItem {
    let theme = cx.theme();
    let palette = Palette::of(cx);
    let item = entry.item();
    let (icon, hue, address) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground, None)
    } else if item.id.starts_with("Imports/") {
        ("sign-in-duotone", palette.external, None)
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

#[cfg(test)]
mod tests {
    use super::{TreeItem, id, items};
    use crate::mock;

    fn labels(items: &[TreeItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_ref()).collect()
    }

    #[test]
    fn builds_the_mock_symbols() {
        let program = mock::program();
        let tree = items(&program);
        assert_eq!(labels(&tree), ["Functions", "Imports"]);
        assert_eq!(
            labels(&tree[0].children),
            [
                "auth",
                "config",
                "fs",
                "net",
                "FUN_00413b10",
                "FUN_00414c88",
                "FUN_00415a20",
                "handle_client",
                "main"
            ]
        );
        assert_eq!(labels(&tree[1].children), ["libc.so.0"]);
        assert_eq!(tree[1].children[0].children.len(), 28);

        let mut ids: Vec<&str> = tree[0]
            .children
            .iter()
            .flat_map(|item| {
                if item.is_folder() {
                    item.children.iter().collect()
                } else {
                    vec![item]
                }
            })
            .map(|item| item.id.as_ref())
            .collect();
        let mut addresses: Vec<String> = program.functions.iter().map(|f| id(f.entry)).collect();
        ids.sort_unstable();
        addresses.sort_unstable();
        assert_eq!(ids, addresses);
    }
}
