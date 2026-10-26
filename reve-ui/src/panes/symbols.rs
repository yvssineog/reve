use std::collections::BTreeMap;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::*;

use super::tree_row;
use crate::palette::Palette;

// TODO: the Ghidra adapter should provide these symbols (functions by namespace with addresses, imports by library) instead of this mock.
#[rustfmt::skip]
const FUNCTIONS: &[(&str, Option<&str>, &str)] = &[
    ("00410c40", None, "main"),
    ("00411f04", None, "handle_client"),
    ("00412a30", Some("net"), "parse_request"),
    ("00412e10", Some("net"), "read_line"),
    ("00413120", Some("net"), "url_decode"),
    ("00413480", Some("net"), "send_response"),
    ("00413900", Some("auth"), "check_session"),
    ("00413b10", None, "FUN_00413b10"),
    ("00413e60", Some("config"), "load_config"),
    ("00414400", Some("fs"), "serve_file"),
    ("004149a0", Some("fs"), "mime_type"),
    ("00414c88", None, "FUN_00414c88"),
    ("00415a20", None, "FUN_00415a20"),
];
const LIBRARY: &str = "libc.so.0";
#[rustfmt::skip]
const IMPORTS: &[&str] = &[
    "accept", "atoi", "bind", "close", "dprintf", "fclose", "fopen", "fread", "getpid",
    "isxdigit", "json_get_int", "json_parse", "listen", "memset", "open", "recv", "send",
    "sendfile", "snprintf", "socket", "sscanf", "strchr", "strcmp", "strlen", "strncasecmp",
    "strncpy", "strrchr", "strstr",
];
// parse_request, the function the app opens on.
const START: &str = "00412a30";

// The tree as it opens: functions shown by namespace, imports folded, the start function selected.
pub fn state(cx: &mut Context<TreeState>) -> TreeState {
    let mut state = TreeState::new(cx).items(items());
    let start = START.into();
    state.reveal_item(&start, ScrollStrategy::Nearest, cx);
    state.set_selected_index(state.index_of(&start), cx);
    state
}

// Namespaces then unscoped functions, each by name; imports under their library.
// Function ids are addresses, which outlive renames. Folder ids end in `/`.
fn items() -> Vec<TreeItem> {
    let mut functions = FUNCTIONS.to_vec();
    functions.sort_unstable_by_key(|&(_, _, name)| name);
    let mut namespaces: BTreeMap<&str, Vec<TreeItem>> = BTreeMap::new();
    let mut unscoped = Vec::new();
    for (address, namespace, name) in functions {
        let leaf = TreeItem::new(address, name);
        match namespace {
            Some(namespace) => namespaces.entry(namespace).or_default().push(leaf),
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

// Functions in the symbol hue with their address. Ghidra calls the ones nobody has named yet
// `FUN_<address>`; those stay muted, so the color shows analysis progress. Imports in the external hue.
pub fn row(ix: usize, entry: &TreeEntry, _: bool, _: &mut Window, cx: &mut App) -> ListItem {
    let theme = cx.theme();
    let palette = Palette::of(cx);
    let item = entry.item();
    let function = FUNCTIONS.iter().find(|(address, ..)| *address == &*item.id);
    let (icon, hue, address) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground, None)
    } else if let Some((address, ..)) = function {
        let hue = if item.label.starts_with("FUN_") {
            palette.fg_3
        } else {
            palette.symbol
        };
        (
            "function-duotone",
            hue,
            Some(address.trim_start_matches('0')),
        )
    } else {
        ("sign-in-duotone", palette.external, None)
    };
    tree_row(ix, entry, icon, hue, address, cx)
}

#[cfg(test)]
mod tests {
    use super::{FUNCTIONS, TreeItem, items};

    fn labels(items: &[TreeItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_ref()).collect()
    }

    #[test]
    fn builds_the_mock_symbols() {
        let tree = items();
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
        let mut addresses: Vec<&str> = FUNCTIONS.iter().map(|&(address, ..)| address).collect();
        ids.sort_unstable();
        addresses.sort_unstable();
        assert_eq!(ids, addresses);
    }
}
