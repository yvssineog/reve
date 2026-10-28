mod empty;
mod header;
mod logo;
mod palette;
mod panes;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui_kit::assets::Assets;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, Theme, ThemeRegistry, TitleBar, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_ghidra_adapter::{GhidraAdapter, GhidraConfig, ParserError};
use reve_model::Program;

use logo::Logo;
use palette::Palette;
use panes::decompile::Decompile;
use panes::functions::Functions;
use panes::listing::Listing;
use panes::symbols::Symbols;
use panes::{note, pane};

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
    "tree-structure-duotone",
    "cpu-duotone",
    "brackets-curly-duotone",
    "function-duotone",
    "sign-in-duotone",
    "sun-duotone",
    "moon-duotone",
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

// A bundled icon at `size`.
fn icon(name: &str, size: Pixels) -> Icon {
    Icon::empty()
        .path(format!("icons/{name}.svg"))
        .with_size(size)
}

// Commands that keyboard shortcuts, buttons and menus can trigger.
actions!(
    reve,
    [
        Quit,
        OpenBinary,
        ToggleSidebar,
        ToggleBottom,
        ToggleAppearance
    ]
);

// The main view: the empty screen until a binary is open, then header, body, footer
// stacked top to bottom.
struct Reve {
    // Takes the app's shortcuts when no pane holds focus.
    focus: FocusHandle,
    sidebar_open: bool,
    bottom_open: bool,
    screen: Screen,
}

// The animated logo until a binary is open, then the workspace showing it. Opening one
// drops the logo, which stops its animation.
enum Screen {
    Empty(Entity<Logo>),
    Open(Workspace),
}

// An open binary and the panes showing it.
struct Workspace {
    // The binary's file name.
    name: SharedString,
    analysis: Analysis,
}

// Where Ghidra is with the binary.
enum Analysis {
    Analyzing,
    Done(Panes),
    // Why, in one line.
    Failed(SharedString),
}

// The analyzed binary, pane by pane.
struct Panes {
    summary: SharedString,
    symbols: Entity<Symbols>,
    listing: Entity<Listing>,
    decompile: Entity<Decompile>,
    functions: Entity<Functions>,
}

impl Reve {
    // Opens on the empty screen, focused so its I key works without a click first.
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            sidebar_open: true,
            bottom_open: true,
            screen: Screen::Empty(cx.new(Logo::new)),
        }
    }

    // Asks the system's file picker for the binary to analyze; cancelling leaves the empty
    // screen.
    fn open_binary(&mut self, _: &OpenBinary, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // ponytail: a picker that fails to open (Linux without a portal) is ignored like a cancel.
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(binary) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.open(binary, window, cx))
                .ok();
        })
        .detach();
    }

    // Shows `binary` and starts analyzing it.
    fn open(&mut self, binary: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // One workspace, one analysis.
        if let Screen::Open(_) = self.screen {
            return;
        }
        let name = SharedString::from(binary.file_name().unwrap().to_string_lossy().into_owned());
        Self::analyze(binary, name.clone(), window, cx);
        self.screen = Screen::Open(Workspace {
            name,
            analysis: Analysis::Analyzing,
        });
        cx.notify();
    }

    // Runs Ghidra on `binary` off the main thread, then shows the result.
    fn analyze(binary: PathBuf, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let analysis = cx.background_spawn(async move {
            GhidraAdapter::new(GhidraConfig::from_environment()).analyze(binary)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = analysis.await;
            this.update_in(cx, |this, window, cx| {
                this.analyzed(name, result, window, cx)
            })
            .ok();
        })
        .detach();
    }

    // Fills the Symbol Tree, Listing, Decompile and Functions, or says why not. The listing
    // takes focus unless the user has already put it in a pane.
    fn analyzed(
        &mut self,
        name: SharedString,
        result: Result<Program, ParserError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Screen::Open(workspace) = &mut self.screen else {
            return;
        };
        workspace.analysis = match result {
            Err(error) => {
                eprintln!("analyzing {name}: {error}");
                Analysis::Failed(match error {
                    ParserError::GhidraFailed { status, .. } => {
                        format!("Ghidra failed ({status})").into()
                    }
                    error => error.to_string().into(),
                })
            }
            // The listing needs an instruction to put its cursor on.
            Ok(program) if program.functions.iter().all(|f| f.instructions.is_empty()) => {
                Analysis::Failed(format!("Ghidra found no code in {name}").into())
            }
            Ok(program) => {
                let summary = format!(
                    "{name} · {} · {} functions",
                    program.architecture,
                    program.functions.len()
                );
                let listing = cx.new(|cx| Listing::new(program, cx));
                let symbols = cx.new(|cx| Symbols::new(listing.clone(), cx));
                let decompile = cx.new(|cx| Decompile::new(listing.clone(), cx));
                let functions = cx.new(|cx| Functions::new(listing.clone(), cx));
                if self.focus.is_focused(window) {
                    window.focus(&listing.focus_handle(cx), cx);
                }
                Analysis::Done(Panes {
                    summary: summary.into(),
                    symbols,
                    listing,
                    decompile,
                    functions,
                })
            }
        };
        cx.notify();
    }
}

impl Render for Reve {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = match &self.screen {
            Screen::Empty(logo) => {
                return empty::screen(logo.clone(), cx)
                    .track_focus(&self.focus)
                    .key_context(empty::CONTEXT)
                    .on_action(cx.listener(Self::open_binary))
                    .into_any_element();
            }
            Screen::Open(workspace) => workspace,
        };
        let header = header::header(self.sidebar_open, self.bottom_open, window, cx);
        let palette = Palette::of(cx);
        let theme = cx.theme();
        let (busy, status, panes) = match &workspace.analysis {
            Analysis::Analyzing => (true, format!("Analyzing {}…", workspace.name).into(), None),
            Analysis::Done(panes) => (false, panes.summary.clone(), Some(panes)),
            Analysis::Failed(why) => (false, why.clone(), None),
        };
        // A pane's view, or a note until the analysis is done.
        let body = |view: fn(&Panes) -> AnyView| match panes {
            Some(panes) => view(panes).into_any_element(),
            None => note("Not ready.", cx).into_any_element(),
        };
        let decompiled = panes.map(|panes| panes.decompile.read(cx).name());

        // The Symbol Tree, on the chrome tint. Keeps its width when the window resizes.
        let sidebar = resizable_panel()
            .visible(self.sidebar_open)
            .size(px(264.))
            .size_range(px(200.)..px(480.))
            .fixed()
            .child(
                v_resizable("sidebar").child(
                    pane(
                        theme,
                        "tree-structure-duotone",
                        palette.symbol,
                        "Symbol Tree",
                        Some(workspace.name.clone()),
                    )
                    .child(body(|panes| panes.symbols.clone().into())),
                ),
            );

        // Listing beside Decompile over the Functions window, on the darker content tint.
        // Functions keeps its height when the window resizes.
        let main = resizable_panel().bg(theme.background).child(
            v_resizable("main")
                .child(
                    resizable_panel().child(
                        h_resizable("code")
                            .child(
                                pane(theme, "cpu-duotone", palette.mnemonic, "Listing", None)
                                    .child(body(|panes| panes.listing.clone().into())),
                            )
                            .child(
                                pane(
                                    theme,
                                    "brackets-curly-duotone",
                                    palette.string,
                                    "Decompile",
                                    decompiled,
                                )
                                .child(body(|panes| panes.decompile.clone().into())),
                            ),
                    ),
                )
                .child(
                    pane(theme, "function-duotone", palette.symbol, "Functions", None)
                        .visible(self.bottom_open)
                        .size(px(210.))
                        .size_range(px(120.)..px(480.))
                        .fixed()
                        .child(body(|panes| panes.functions.clone().into())),
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
            // Footer: the analysis status, the one place it shows.
            .child(
                h_flex()
                    .h(px(28.))
                    .px_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(palette.fg_3)
                    .when(busy, |footer| {
                        footer.child(Spinner::new().with_size(px(12.)).color(palette.fg_3))
                    })
                    .child(div().min_w_0().truncate().child(status)),
            )
            .into_any_element()
    }
}

// A shortcut; "secondary" means Cmd on macOS and Ctrl elsewhere.
fn key(keys: &str, action: impl Action, cx: &App) -> KeyBinding {
    KeyBinding::load(keys, Box::new(action), None, false, None, cx).unwrap()
}

fn main() {
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        // Set up gpui-kit (theme, components). Must run first.
        gpui_kit::init(cx);
        panes::listing::init(cx);

        // Dark is warm greys from oklch(0.203 0.0032 17.4) up, not the kit's near-black.
        ThemeRegistry::global_mut(cx)
            .load_themes_from_str(include_str!("../assets/themes/dark.json"))
            .unwrap();
        Theme::global_mut(cx).dark_theme = ThemeRegistry::global(cx).themes()["rêve Dark"].clone();

        // Start in the system's light or dark mode.
        Theme::sync_system_appearance(None, cx);

        // Keyboard shortcuts.
        let keys = [
            key("secondary-q", Quit, cx),
            key("secondary-b", ToggleSidebar, cx),
            key("secondary-j", ToggleBottom, cx),
            KeyBinding::new("i", OpenBinary, Some(empty::CONTEXT)),
        ];
        cx.bind_keys(keys);

        // Quit from the shortcut or the app menu.
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus([Menu::new("rêve").items([MenuItem::action("Quit rêve", Quit)])]);

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
            window.set_window_title("rêve");

            // Switch light/dark live when the system setting changes.
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx)
                })
                .detach();
            cx.new(|cx| Reve::new(window, cx))
        })
        .expect("failed to open window");

        #[cfg(target_os = "macos")]
        logo::set_dock_icon();

        // Bring the app to the front.
        cx.activate(true);
    });
}
