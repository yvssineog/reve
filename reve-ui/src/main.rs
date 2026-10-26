mod header;
mod project;
mod symbols;

use std::borrow::Cow;

use gpui_kit::assets::Assets;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::resizable::{ResizablePanel, h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::tree::{TreeEntry, TreeState, tree};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, Theme, TitleBar, h_flex, v_flex,
};
use gpui_kit::*;

// Icons bundled into the binary: Phosphor (MIT), the set the Electron build uses,
// plus the panel toggle glyphs.
macro_rules! icons {
    ($($name:literal),+) => {
        &[$((
            concat!("icons/", $name, ".svg"),
            include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8],
        )),+]
    };
}
const ICONS: &[(&str, &[u8])] = icons![
    "caret-left",
    "caret-right",
    "caret-down",
    "magnifying-glass-bold",
    "folder-simple-duotone",
    "file-duotone",
    "binary-duotone",
    "tree-structure-duotone",
    "cpu-duotone",
    "brackets-curly-duotone",
    "function-duotone",
    "sign-in-duotone",
    "sun",
    "moon",
    "panel-open",
    "panel-closed"
];

// gpui-kit's default assets plus our icons.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ICONS.iter().find(|(name, _)| *name == path) {
            Some((_, svg)) => Ok(Some(Cow::Borrowed(svg))),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(ICONS.iter().map(|(name, _)| SharedString::from(*name)));
        Ok(paths)
    }
}

// Commands that keyboard shortcuts, buttons and menus can trigger.
actions!(reve, [Quit, ToggleSidebar, ToggleBottom, ToggleAppearance]);

// The main view: header, body, footer stacked top to bottom.
struct Reve {
    // Lets the view receive keyboard shortcuts.
    focus: FocusHandle,
    sidebar_open: bool,
    bottom_open: bool,
    project: Entity<TreeState>,
    symbols: Entity<TreeState>,
}

impl Reve {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            sidebar_open: true,
            bottom_open: true,
            project: cx.new(project::state),
            symbols: cx.new(symbols::state),
        }
    }
}

impl Render for Reve {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = header::header(self.sidebar_open, self.bottom_open, window, cx);
        let theme = cx.theme();

        // Project above Symbol Tree, on the chrome tint. Keeps its width when the window resizes.
        let sidebar = resizable_panel()
            .visible(self.sidebar_open)
            .size(px(264.))
            .size_range(px(200.)..px(480.))
            .fixed()
            .child(
                v_resizable("sidebar")
                    .child(
                        pane(
                            theme,
                            "folder-simple",
                            theme.cyan,
                            "Project",
                            Some(project::NAME),
                        )
                        .child(
                            // Inset so the selected row's rounded fill clears the pane edges.
                            tree(&self.project, project::row)
                                .flex_1()
                                .min_h_0()
                                .px(px(6.)),
                        ),
                    )
                    .child(
                        pane(theme, "tree-structure", theme.magenta, "Symbol Tree", None).child(
                            tree(&self.symbols, symbols::row)
                                .flex_1()
                                .min_h_0()
                                .px(px(6.)),
                        ),
                    ),
            );

        // Listing beside Decompile over the Functions window, on the darker content tint.
        // Functions keeps its height when the window resizes.
        let main = resizable_panel().bg(theme.background).child(
            v_resizable("main")
                .child(
                    resizable_panel().child(
                        h_resizable("code")
                            .child(pane(theme, "cpu", theme.blue, "Listing", None))
                            .child(pane(
                                theme,
                                "brackets-curly",
                                theme.green,
                                "Decompile",
                                None,
                            )),
                    ),
                )
                .child(
                    pane(theme, "function", theme.magenta, "Functions", None)
                        .visible(self.bottom_open)
                        .size(px(210.))
                        .size_range(px(120.)..px(480.))
                        .fixed(),
                ),
        );

        v_flex()
            .size_full()
            .bg(theme.title_bar)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleBottom, _, cx| {
                this.bottom_open = !this.bottom_open;
                cx.notify();
            }))
            .child(header)
            // Dividers between panes are hairlines that can be dragged.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(h_resizable("body").child(sidebar).child(main)),
            )
            // Footer: the status line will go here.
            .child(div().h(px(28.)).border_t_1().border_color(theme.border))
    }
}

// A pane: a header row (Phosphor duotone icon in the pane's hue + title + optional detail)
// over the caller's body.
fn pane(
    theme: &Theme,
    icon: &str,
    hue: Hsla,
    title: &'static str,
    detail: Option<&'static str>,
) -> ResizablePanel {
    resizable_panel().flex_col().child(
        h_flex()
            .h(px(32.))
            .flex_none()
            .px_3()
            .gap_2()
            .text_xs()
            .font_medium()
            .text_color(theme.muted_foreground)
            .child(
                Icon::empty()
                    .path(format!("icons/{icon}-duotone.svg"))
                    .with_size(px(14.))
                    .text_color(hue),
            )
            .child(title)
            // Detail reads quieter than the title, in the same font and size so baselines line up.
            .children(detail.map(|detail| {
                div()
                    .min_w_0()
                    .truncate()
                    .font_normal()
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child(detail)
            })),
    )
}

// A compact tree row: indent, caret, icon in its hue, the label, then an optional detail on
// the right. When narrow the label gives way first, cut with an ellipsis.
fn tree_row(
    ix: usize,
    entry: &TreeEntry,
    icon: &str,
    hue: Hsla,
    detail: Option<&'static str>,
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
                .child(div().min_w_0().truncate().child(entry.item().label.clone()))
                // Faint monospace at the label's size, so baselines line up.
                .children(detail.map(|detail| {
                    div()
                        .ml_auto()
                        .pl_2()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground.opacity(0.7))
                        .child(detail)
                })),
        )
}

// A shortcut; "secondary" means Cmd on macOS and Ctrl elsewhere.
fn key(keys: &str, action: impl Action, cx: &App) -> KeyBinding {
    KeyBinding::load(keys, Box::new(action), None, false, None, cx).unwrap()
}

fn main() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        // Set up gpui-kit (theme, components). Must run first.
        gpui_kit::init(cx);

        // Start in the system's light or dark mode.
        Theme::sync_system_appearance(None, cx);

        // Keyboard shortcuts.
        let keys = [
            key("secondary-q", Quit, cx),
            key("secondary-b", ToggleSidebar, cx),
            key("secondary-j", ToggleBottom, cx),
        ];
        cx.bind_keys(keys);

        // Quit from the shortcut or the app menu.
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus([Menu::new("Rêve").items([MenuItem::action("Quit Rêve", Quit)])]);

        // Flips light/dark. On macOS this also re-themes the native parts, like the
        // traffic lights and menus, and stops following the system until relaunch.
        cx.on_action(|_: &ToggleAppearance, cx| {
            let next = if cx.theme().mode.is_dark() {
                WindowAppearance::Light
            } else {
                WindowAppearance::Dark
            };
            cx.set_window_appearance(Some(next));
            Theme::change(next, None, cx);
        });

        // Also quit when the last window is closed.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        // Open a 1440x900 window in the middle of the screen.
        // The system title bar is hidden; our header draws in its place.
        // The traffic lights sit where the Electron build has them, centered in the 42px header.
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
            window_min_size: Some(size(px(1024.), px(600.))),
            titlebar: Some(TitlebarOptions {
                traffic_light_position: Some(point(px(16.), px(15.))),
                ..TitleBar::title_bar_options()
            }),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            // Name shown in Mission Control and the Window menu.
            window.set_window_title("Rêve");

            // Switch light/dark live when the system setting changes.
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx)
                })
                .detach();
            cx.new(|cx| Reve::new(window, cx))
        })
        .expect("failed to open window");

        // Bring the app to the front.
        cx.activate(true);
    });
}
