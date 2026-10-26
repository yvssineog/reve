use std::collections::BTreeMap;

use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Binary,
    Text,
}

// Mock project: its name and the flat file table the engine will produce.
pub const NAME: &str = "Halo AX3000 1.4.2";
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

// A compact row: indent, caret, icon, then the name, cut with an ellipsis when narrow.
pub fn row(ix: usize, entry: &TreeEntry, _: bool, _: &mut Window, cx: &mut App) -> ListItem {
    let theme = cx.theme();
    let item = entry.item();
    let (icon, hue) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground)
    } else if FILES.contains(&(&*item.id, Kind::Binary)) {
        ("binary-duotone", theme.cyan)
    } else {
        ("file-duotone", theme.muted_foreground)
    };
    let caret = match (entry.is_folder(), entry.is_expanded()) {
        (false, _) => None,
        (true, false) => Some("caret-right"),
        (true, true) => Some("caret-down"),
    };

    ListItem::new(ix)
        .h(px(24.))
        .py_0()
        .px(px(6.))
        .rounded(px(6.))
        .text_size(px(13.))
        .child(
            h_flex()
                .gap_1()
                .pl(px(16.) * entry.depth() as f32)
                // Files keep an empty caret slot so their icons line up with folders'.
                .child(div().flex_none().size(px(12.)).children(caret.map(|caret| {
                    Icon::empty()
                        .path(format!("icons/{caret}.svg"))
                        .with_size(px(12.))
                        .text_color(theme.muted_foreground)
                })))
                .child(
                    Icon::empty()
                        .path(format!("icons/{icon}.svg"))
                        .with_size(px(14.))
                        .text_color(hue),
                )
                .child(div().min_w_0().truncate().child(item.label.clone())),
        )
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
