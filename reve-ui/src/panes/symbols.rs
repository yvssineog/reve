use std::time::Duration;

use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeEntry, TreeItem, TreeState, tree};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_model::{Address, Program};

use super::listing::Listing;
use crate::icon;
use crate::palette::Palette;

// How long the cursor rests in a function before the tree marks it.
const SETTLE: Duration = Duration::from_millis(50);

// Every function by name, the one the listing cursor is in marked and in view.
pub struct Symbols {
    listing: Entity<Listing>,
    tree: Entity<TreeState>,
    // The listing cursor's function, marked or about to be.
    current: Address,
    marking: Option<Task<()>>,
    _sync: Subscription,
}

impl Symbols {
    // Opens with the functions shown and the listing cursor's marked, then follows the cursor.
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let (items, entry) = {
            let listing = listing.read(cx);
            (items(listing.program()), listing.function().entry)
        };
        let tree = cx.new(|cx| {
            let mut tree = TreeState::new(cx).items(items);
            mark(&mut tree, entry, cx);
            tree
        });
        let sync = cx.observe(&listing, |this, listing, cx| {
            let entry = listing.read(cx).function().entry;
            if entry != this.current {
                this.current = entry;
                this.marking = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SETTLE).await;
                    this.update(cx, |this, cx| this.mark(cx)).ok();
                }));
            }
        });
        Self {
            listing,
            tree,
            current: entry,
            marking: None,
            _sync: sync,
        }
    }

    // Marks the cursor's function once the cursor rests there. Marking jumps the tree to
    // another run of names, all new text to lay out; held arrows would pay for it every step
    // and the tree would flicker through names nobody can read.
    fn mark(&mut self, cx: &mut Context<Self>) {
        let entry = self.current;
        self.tree.update(cx, |tree, cx| mark(tree, entry, cx));
    }
}

impl Render for Symbols {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Inset so the selected row's rounded fill clears the pane edges.
        tree(&self.tree, row(self.listing.clone()))
            .size_full()
            .px(px(6.))
    }
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

// Functions in their name's hue with their address. Pressing one takes every pane to it.
fn row(
    listing: Entity<Listing>,
) -> impl Fn(usize, &TreeEntry, bool, &mut Window, &mut App) -> ListItem + 'static {
    move |ix, entry, selected, _, cx| {
        let item = entry.item();
        if entry.is_folder() {
            let hue = cx.theme().muted_foreground;
            return tree_row(ix, entry, "folder-simple-duotone", hue, None, cx);
        }
        let hue = Palette::of(cx).name(&item.label);
        let address = item.id.trim_start_matches('0').to_owned();
        let target = u64::from_str_radix(&item.id, 16).ok().map(Address);
        let listing = listing.clone();
        tree_row(ix, entry, "function-duotone", hue, Some(address.into()), cx)
            .selected(selected)
            .when_some(target, |row, entry| {
                row.on_click(move |_, _, cx| {
                    listing.update(cx, |listing, cx| {
                        listing.go_to(entry, ScrollStrategy::Center, cx)
                    })
                })
            })
    }
}

// A compact tree row: indent, caret, icon in its hue, the label, then an optional detail on
// the right. When narrow the label gives way first, cut with an ellipsis.
fn tree_row(
    ix: usize,
    entry: &TreeEntry,
    glyph: &str,
    hue: Hsla,
    detail: Option<SharedString>,
    cx: &App,
) -> ListItem {
    let theme = cx.theme();
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
                // Leaves keep an empty caret slot so their icons line up with folders'.
                .child(div().flex_none().size(px(12.)).children(
                    caret.map(|caret| icon(caret, px(12.)).text_color(theme.muted_foreground)),
                ))
                .child(icon(glyph, px(14.)).text_color(hue))
                .child(div().min_w_0().truncate().child(entry.item().label.clone()))
                // Faint monospace at the label's size, so baselines line up. Never wraps or
                // shrinks; the label gives way instead.
                .children(detail.map(|detail| {
                    div()
                        .flex_none()
                        .ml_auto()
                        .pl_2()
                        .whitespace_nowrap()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground.opacity(0.7))
                        .child(detail)
                })),
        )
}
