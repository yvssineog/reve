use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read as _;
use std::path::Path;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState};
use gpui_kit::*;

use super::tree_row;
use crate::palette::Palette;

// A file under the open folder: its `/`-separated path from the folder, and whether it's ELF.
pub struct File {
    pub path: String,
    pub elf: bool,
}

// Every file under `folder`, by path. Hidden entries are left out, symlinks aren't followed,
// unreadable folders are skipped, and a file whose header can't be read counts as not ELF.
// ponytail: walks the whole folder before the workspace opens, so a huge one (a home
// directory) holds the empty screen; stream the walk if that bites.
pub fn scan(folder: &Path) -> Vec<File> {
    let mut files = Vec::new();
    walk(folder, "", &mut files);
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    files
}

fn walk(dir: &Path, prefix: &str, files: &mut Vec<File>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{prefix}{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&entry.path(), &format!("{path}/"), files),
            Ok(_) => files.push(File {
                elf: is_elf(&entry.path()),
                path,
            }),
            Err(_) => {}
        }
    }
}

fn is_elf(path: &Path) -> bool {
    let mut magic = [0; 4];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok()
        && magic == *b"\x7fELF"
}

// The binary to analyze: the shallowest ELF, the first by path among equals.
pub fn target(files: &[File]) -> Option<&File> {
    files
        .iter()
        .filter(|file| file.elf)
        .min_by_key(|file| file.path.matches('/').count())
}

// The tree as it opens: the analyzed binary, if any, revealed and selected.
pub fn state(files: &[File], target: Option<&File>, cx: &mut Context<TreeState>) -> TreeState {
    let paths = files.iter().map(|file| file.path.as_str()).collect();
    let mut state = TreeState::new(cx).items(items("", paths));
    if let Some(target) = target {
        let id = SharedString::from(target.path.clone());
        state.reveal_item(&id, ScrollStrategy::Nearest, cx);
        state.set_selected_index(state.index_of(&id), cx);
    }
    state
}

// One level of the tree from paths relative to `dir`, folders before files, each by name.
// Ids are full paths; folder ids end in `/` so they never collide with a file's.
fn items(dir: &str, paths: Vec<&str>) -> Vec<TreeItem> {
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

// ELF files, by id, stand out in the number hue; folders and other files stay muted.
pub fn row(elf_files: &HashSet<SharedString>, ix: usize, entry: &TreeEntry, cx: &App) -> ListItem {
    let theme = cx.theme();
    let (icon, hue) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground)
    } else if elf_files.contains(&entry.item().id) {
        ("binary-duotone", Palette::of(cx).number)
    } else {
        ("file-duotone", theme.muted_foreground)
    };
    tree_row(ix, entry, icon, hue, None, cx)
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::{TreeItem, items, scan, target};

    fn labels(items: &[TreeItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_ref()).collect()
    }

    #[test]
    fn collapses_folders_holding_only_a_folder() {
        let tree = items("", vec!["root/usr/sbin/httpd", "root/bin/sh", "notes.md"]);
        assert_eq!(labels(&tree), ["root", "notes.md"]);
        assert_eq!(labels(&tree[0].children), ["bin", "usr/sbin"]);
        assert_eq!(&*tree[0].children[1].id, "root/usr/sbin/");
        assert_eq!(&*tree[0].children[1].children[0].id, "root/usr/sbin/httpd");
    }

    #[test]
    fn scans_files_and_picks_the_shallowest_elf() {
        let root = env::temp_dir().join(format!("reve-scan-{}", process::id()));
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("bin/tool"), b"\x7fELF\x02\x01").unwrap();
        fs::write(root.join("zapp"), b"\x7fELF\x01\x01").unwrap();
        fs::write(root.join("notes.txt"), b"\x7fEL").unwrap();
        fs::write(root.join(".hidden"), b"\x7fELF").unwrap();
        fs::write(root.join(".git/HEAD"), b"ref").unwrap();

        let files = scan(&root);
        fs::remove_dir_all(&root).unwrap();

        let listed: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.elf)).collect();
        assert_eq!(
            listed,
            [("bin/tool", true), ("notes.txt", false), ("zapp", true)]
        );
        assert_eq!(target(&files).map(|f| f.path.as_str()), Some("zapp"));
    }
}
