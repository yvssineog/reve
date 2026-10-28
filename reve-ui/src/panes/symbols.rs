use std::time::Duration;

use gpui_kit::base::{ScrollBounce, Tree, TreeEntry, TreeItem, TreeState};
use gpui_kit::component::scroll::ScrollableElement as _;
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scroll = self.tree.read(cx).scroll_handle().clone();
        let listing = self.listing.clone();
        let tree = Tree::new(&self.tree)
            .item(move |ix, entry, state, _, cx| row(ix, entry, state.is_selected(), &listing, cx))
            .list_style(StyleRefinement::default().flex_grow_1().size_full())
            .relative()
            .size_full();
        // Inset so the selected row's rounded fill clears the pane edges.
        div()
            .id("symbols")
            .size_full()
            .px(px(6.))
            .child(ScrollBounce::new("symbols-bounce", &scroll, tree))
            .vertical_scrollbar(&scroll)
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

// A compact row: indent, caret, icon in its hue, the label, then the function's address on
// the right. When narrow the label gives way first, cut with an ellipsis. Functions take their
// name's hue; pressing one takes every pane to it.
fn row(
    ix: usize,
    entry: &TreeEntry,
    selected: bool,
    listing: &Entity<Listing>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let item = entry.item();
    let caret = match (entry.is_folder(), entry.is_expanded()) {
        (false, _) => None,
        (true, false) => Some("caret-right"),
        (true, true) => Some("caret-down"),
    };
    let (glyph, hue, target) = if entry.is_folder() {
        ("folder-simple-duotone", theme.muted_foreground, None)
    } else {
        let target = u64::from_str_radix(&item.id, 16).ok().map(Address);
        (
            "function-duotone",
            Palette::of(cx).name(&item.label),
            target,
        )
    };
    let selection = if theme.list.active_highlight {
        theme.list_active
    } else {
        theme.accent
    };

    h_flex()
        .id(ix)
        .h(px(24.))
        .px(px(6.))
        .rounded(px(6.))
        .text_size(px(13.))
        .text_color(theme.foreground)
        .map(|row| {
            if selected {
                row.bg(selection)
            } else {
                row.hover(|row| row.bg(theme.tokens.list_hover))
            }
        })
        .child(
            h_flex()
                .w_full()
                .gap_1()
                .pl(px(16.) * entry.depth() as f32)
                // Leaves keep an empty caret slot so their icons line up with folders'.
                .child(div().flex_none().size(px(12.)).children(
                    caret.map(|caret| icon(caret, px(12.)).text_color(theme.muted_foreground)),
                ))
                .child(icon(glyph, px(14.)).text_color(hue))
                .child(div().min_w_0().truncate().child(item.label.clone()))
                // Faint monospace at the label's size, so baselines line up. Never wraps or
                // shrinks; the label gives way instead.
                .when(target.is_some(), |row| {
                    row.child(
                        div()
                            .flex_none()
                            .ml_auto()
                            .pl_2()
                            .whitespace_nowrap()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground.opacity(0.7))
                            .child(item.id.trim_start_matches('0').to_owned()),
                    )
                }),
        )
        .when_some(target, |row, entry| {
            let listing = listing.clone();
            row.on_click(move |_, _, cx| listing.update(cx, |listing, cx| listing.go_to(entry, cx)))
        })
        .into_any_element()
}
