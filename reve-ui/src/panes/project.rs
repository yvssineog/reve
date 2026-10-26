use std::collections::BTreeMap;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::*;

use super::tree_row;
use crate::palette::Palette;

// TODO: the Ghidra adapter should classify each file (container, executable, library, script, text) instead of this hand-labelled table.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Binary,
    Text,
}

// Mock project: the flat file table the engine will produce.
const ANALYZED: &str = "_firmware.bin.extracted/squashfs-root/usr/sbin/httpd";
#[rustfmt::skip]
const FILES: &[(&str, Kind)] = &[
    ("firmware.bin", Kind::Binary),
    ("notes.md", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/bin/busybox", Kind::Binary),
    ("_firmware.bin.extracted/squashfs-root/etc/config.json", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/etc/init.d/rcS", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/etc/version", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/lib/libc.so.0", Kind::Binary),
    (ANALYZED, Kind::Binary),
    ("_firmware.bin.extracted/squashfs-root/www/index.html", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/www/js/app.js", Kind::Text),
    ("_firmware.bin.extracted/squashfs-root/www/css/app.css", Kind::Text),
];

// The tree as it opens: the analyzed binary revealed and selected.
pub fn state(cx: &mut Context<TreeState>) -> TreeState {
    let paths = FILES.iter().map(|&(path, _)| path).collect();
    let mut state = TreeState::new(cx).items(items("", paths));
    let analyzed = ANALYZED.into();
    state.reveal_item(&analyzed, ScrollStrategy::Nearest, cx);
    state.set_selected_index(state.index_of(&analyzed), cx);
    state
}

// One level of the tree from paths relative to `dir`, folders before files, each by name.
// Ids are full paths; folder ids end in `/` so they never collide with a file's.
fn items(dir: &str, paths: Vec<&'static str>) -> Vec<TreeItem> {
    let mut folders: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut files = Vec::new();
    for path in paths {
        match path.split_once('/') {
            Some((name, rest)) => folders.entry(name).or_default().push(rest),
            None => files.push(path),
        }
    }
    files.sort_unstable();

    let folders = folders.into_iter().map(|(name, rest)| {
        let id = format!("{dir}{name}/");
        let mut children = items(&id, rest);
        // A folder holding only a folder shares its row, like `usr/sbin`.
        if children.len() == 1 && children[0].is_folder() {
            let only = children.remove(0);
            return TreeItem::new(only.id, format!("{name}/{}", only.label))
                .children(only.children);
        }
        TreeItem::new(id, name).children(children)
    });
    let files = files
        .into_iter()
        .map(|name| TreeItem::new(format!("{dir}{name}"), name));
    folders.chain(files).collect()
}

// Binaries stand out in the number hue; folders and other files stay muted.
pub fn row(ix: usize, entry: &TreeEntry, _: bool, _: &mut Window, cx: &mut App) -> ListItem {
    let theme = cx.theme();
    let (icon, hue) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground)
    } else if FILES.contains(&(&*entry.item().id, Kind::Binary)) {
        ("binary-duotone", Palette::of(cx).number)
    } else {
        ("file-duotone", theme.muted_foreground)
    };
    tree_row(ix, entry, icon, hue, None, cx)
}

#[cfg(test)]
mod tests {
    use super::{FILES, TreeItem, items};

    fn labels(items: &[TreeItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_ref()).collect()
    }

    fn leaves(items: &[TreeItem]) -> Vec<&str> {
        items
            .iter()
            .flat_map(|item| {
                if item.is_folder() {
                    leaves(&item.children)
                } else {
                    vec![item.id.as_ref()]
                }
            })
            .collect()
    }

    #[test]
    fn builds_the_mock_project() {
        let mut paths: Vec<_> = FILES.iter().map(|&(path, _)| path).collect();
        let tree = items("", paths.clone());
        assert_eq!(
            labels(&tree),
            [
                "_firmware.bin.extracted/squashfs-root",
                "firmware.bin",
                "notes.md"
            ]
        );
        assert_eq!(
            labels(&tree[0].children),
            ["bin", "etc", "lib", "usr/sbin", "www"]
        );

        let mut ids = leaves(&tree);
        ids.sort_unstable();
        paths.sort_unstable();
        assert_eq!(ids, paths);
    }
}
