mod commands;
mod empty;
mod go_to;
mod header;
mod hud;
mod logo;
mod menu;
mod palette;
mod panes;
mod window;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui_kit::assets::Assets;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, Theme, ThemeRegistry, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use reve_ghidra_adapter::{GhidraAdapter, GhidraConfig, ParserError};
use reve_model::Program;

use header::Header;
use hud::Hud;
use logo::Logo;
use menu::Section;
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
    "magnifying-glass",
    "magnifying-glass-bold",
    "folder-simple-duotone",
    "tree-structure-duotone",
    "cpu-duotone",
    "brackets-curly-duotone",
    "function-duotone",
    "sign-in-duotone",
    "sun-duotone",
    "moon-duotone",
    "gauge-duotone",
    "sign-out-duotone",
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
        OpenBinaryInNewWindow,
        CloseBinary,
        ToggleSidebar,
        ToggleBottom,
        ToggleAppearance,
        ToggleFrameStats,
        GoTo,
        GoBack,
        GoForward,
        ShowCommands
    ]
);

// The open workspace's key context. Its one-letter shortcuts stop at dialogs over it, so
// typing in them still types.
const WORKSPACE: &str = "Workspace";

// The main view: the empty screen until a binary is open, then header, body, footer
// stacked top to bottom.
struct Reve {
    // Takes the app's shortcuts when no pane holds focus.
    focus: FocusHandle,
    // Also holds which panels are open.
    header: Entity<Header>,
    screen: Screen,
    // Frame stats on the top bar, when shown.
    hud: Option<Hud>,
}

// The animated logo until a binary is open, then the workspace showing it. Opening one
// drops the logo, which stops its animation.
enum Screen {
    Empty(Entity<Logo>),
    Open(Workspace),
}

// An open binary and the panes showing it.
struct Workspace {
    // The selected file, retained for recent-file and reload actions.
    path: PathBuf,
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
    symbols: Entity<Symbols>,
    listing: Entity<Listing>,
    decompile: Entity<Decompile>,
    functions: Entity<Functions>,
    // Lights the header's Back and Forward while the listing has jumps to walk.
    _history: Subscription,
}

impl Reve {
    // Opens on the empty screen, focused so its I key works without a click first.
    fn new(window: &mut Window, cx: &mut Context<Self>, initial_binary: Option<PathBuf>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            focus,
            header: cx.new(|_| Header {
                sidebar_open: true,
                bottom_open: true,
                back: false,
                forward: false,
            }),
            screen: Screen::Empty(cx.new(Logo::new)),
            // Shown in debug builds; secondary-alt-p toggles them in any build.
            hud: cfg!(debug_assertions).then(Hud::default),
        };
        if let Some(binary) = initial_binary {
            this.open(binary, window, cx);
        }
        this
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

    // Picks a binary and opens it in a separate, independently initialized window.
    fn open_binary_in_new_window(
        &mut self,
        _: &OpenBinaryInNewWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open in New Window".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(binary) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |_, _, cx| {
                if let Err(error) = crate::window::open(cx, Some(binary.clone())) {
                    eprintln!("opening {} in a new window: {error}", binary.display());
                }
            })
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
        let name = SharedString::from(
            binary
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| binary.display().to_string()),
        );
        let path = binary.clone();
        Self::analyze(binary, name.clone(), window, cx);
        self.screen = Screen::Open(Workspace {
            path,
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

    // Drops this window's binary panes and returns it to the home screen.
    fn close_binary(&mut self, _: &CloseBinary, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(&self.screen, Screen::Open(_)) {
            self.screen = Screen::Empty(cx.new(Logo::new));
            self.header.update(cx, |header, cx| {
                header.sidebar_open = true;
                header.bottom_open = true;
                header.back = false;
                header.forward = false;
                cx.notify();
            });
            cx.notify();
        }
    }

    // What the command palette offers on this screen, worded for how things stand. Glyphs
    // take the header tools' ink; the appearance glyph keeps its hue.
    fn commands(&self, window: &Window, cx: &App) -> Vec<Section> {
        let ink = cx.theme().foreground.opacity(0.64);
        let glyph = |name| icon(name, menu::GLYPH).text_color(ink);
        let command = |icon, label, action| commands::row(icon, label, action, window, cx);
        let (mut navigate, mut view, mut reve) = (Vec::new(), Vec::new(), Vec::new());
        match &self.screen {
            Screen::Empty(_) => reve.push(command(
                glyph("folder-simple-duotone"),
                "Open binary",
                Box::new(OpenBinary),
            )),
            Screen::Open(_) => {
                if self.listing().is_some() {
                    navigate = vec![
                        command(
                            glyph("magnifying-glass"),
                            "Go to address or symbol",
                            Box::new(GoTo),
                        ),
                        command(glyph("caret-left"), "Go back", Box::new(GoBack)),
                        command(glyph("caret-right"), "Go forward", Box::new(GoForward)),
                    ];
                }
                let header = self.header.read(cx);
                view.extend([
                    command(
                        header::sidebar_glyph(header.sidebar_open, menu::GLYPH).text_color(ink),
                        header::sidebar_label(header.sidebar_open),
                        Box::new(ToggleSidebar),
                    ),
                    command(
                        header::bottom_glyph(header.bottom_open, menu::GLYPH).text_color(ink),
                        header::bottom_label(header.bottom_open),
                        Box::new(ToggleBottom),
                    ),
                ]);
            }
        }
        let dark = cx.theme().mode.is_dark();
        let stats = if self.hud.is_some() {
            "Hide frame stats"
        } else {
            "Show frame stats"
        };
        view.extend([
            command(
                header::appearance_glyph(dark, menu::GLYPH),
                header::appearance_label(dark),
                Box::new(ToggleAppearance),
            ),
            command(glyph("gauge-duotone"), stats, Box::new(ToggleFrameStats)),
        ]);
        reve.push(command(
            glyph("sign-out-duotone"),
            "Quit rêve",
            Box::new(Quit),
        ));
        vec![
            Section {
                heading: "Navigate",
                rows: navigate,
            },
            Section {
                heading: "View",
                rows: view,
            },
            Section {
                heading: "rêve",
                rows: reve,
            },
        ]
    }

    // The listing, once Ghidra is done with the binary.
    fn listing(&self) -> Option<Entity<Listing>> {
        match &self.screen {
            Screen::Open(Workspace {
                analysis: Analysis::Done(panes),
                ..
            }) => Some(panes.listing.clone()),
            _ => None,
        }
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
        let path = workspace.path.clone();
        workspace.analysis = match result {
            Err(error) => {
                eprintln!("analyzing {}: {error}", path.display());
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
                let listing = cx.new(|cx| Listing::new(program, cx));
                let symbols = cx.new(|cx| Symbols::new(listing.clone(), cx));
                let decompile = cx.new(|cx| Decompile::new(listing.clone(), cx));
                let functions = cx.new(|cx| Functions::new(listing.clone(), cx));
                let history = cx.observe(&listing, |this, listing, cx| {
                    let listing = listing.read(cx);
                    let walkable = (listing.can_go_back(), listing.can_go_forward());
                    this.header.update(cx, |header, cx| {
                        if (header.back, header.forward) != walkable {
                            (header.back, header.forward) = walkable;
                            cx.notify();
                        }
                    });
                });
                if self.focus.is_focused(window) {
                    window.focus(&listing.focus_handle(cx), cx);
                }
                Analysis::Done(Panes {
                    symbols,
                    listing,
                    decompile,
                    functions,
                    _history: history,
                })
            }
        };
        cx.notify();
    }
}

impl Render for Reve {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Before the screen renders, so the frame's time covers it.
        let hud = self.hud.as_mut().map(|hud| hud.frame(window, cx));
        div()
            .size_full()
            .relative()
            .on_action(cx.listener(|this, _: &ShowCommands, window, cx| {
                commands::open(this.commands(window, cx), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleFrameStats, _, cx| {
                this.hud = match this.hud {
                    Some(_) => None,
                    None => Some(Hud::default()),
                };
                cx.notify();
            }))
            .on_action(cx.listener(Self::open_binary_in_new_window))
            .on_action(cx.listener(Self::close_binary))
            .child(self.screen(cx))
            .children(hud)
    }
}

impl Reve {
    // The empty screen until a binary is open, then the workspace.
    fn screen(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
        let &Header {
            sidebar_open,
            bottom_open,
            ..
        } = self.header.read(cx);
        let palette = Palette::of(cx);
        let theme = cx.theme();
        let panes = match &workspace.analysis {
            Analysis::Done(panes) => Some(panes),
            _ => None,
        };
        // A pane's view under its header, or a note until the analysis is done. Each view is
        // cached, so a frame redraws only the panes that changed.
        let body = |view: fn(&Panes) -> AnyView| match panes {
            Some(panes) => view(panes)
                .cached(StyleRefinement::default().flex_1().min_h_0().w_full())
                .into_any_element(),
            None => note("Not ready.", cx).into_any_element(),
        };
        let decompiled = panes.map(|panes| panes.decompile.read(cx).name());
        // Footer text that gives way first when the window is narrow.
        let line = |text: SharedString| div().min_w_0().truncate().child(text);

        // The Symbol Tree, on the chrome tint. Keeps its width when the window resizes.
        let sidebar = resizable_panel()
            .visible(sidebar_open)
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
                        .visible(bottom_open)
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
            .key_context(WORKSPACE)
            .on_action(cx.listener(|this, _: &GoTo, window, cx| {
                if let Some(listing) = this.listing() {
                    go_to::open(listing, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| {
                if let Some(listing) = this.listing() {
                    listing.update(cx, Listing::back);
                }
            }))
            .on_action(cx.listener(|this, _: &GoForward, _, cx| {
                if let Some(listing) = this.listing() {
                    listing.update(cx, Listing::forward);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.header.update(cx, |header, cx| {
                    header.sidebar_open = !header.sidebar_open;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleBottom, _, cx| {
                this.header.update(cx, |header, cx| {
                    header.bottom_open = !header.bottom_open;
                    cx.notify();
                })
            }))
            // Cached like the panes; the window redraws its header only when it changes.
            .child(
                AnyView::from(self.header.clone()).cached(
                    StyleRefinement::default()
                        .w_full()
                        .h(header::HEIGHT)
                        .flex_none(),
                ),
            )
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
                    .map(|footer| match &workspace.analysis {
                        Analysis::Analyzing => footer
                            .child(Spinner::new().with_size(px(12.)).color(palette.fg_3))
                            .child(line(format!("Analyzing {}…", workspace.name).into())),
                        // The binary, its architecture and its function count, each its own field.
                        Analysis::Done(panes) => {
                            let program = panes.listing.read(cx).program();
                            footer
                                .gap_4()
                                .child(line(workspace.name.clone()))
                                .child(
                                    div()
                                        .flex_none()
                                        .font_family(theme.mono_font_family.clone())
                                        .child(program.architecture.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .child(format!("{} functions", program.functions.len())),
                                )
                        }
                        Analysis::Failed(why) => footer.child(line(why.clone())),
                    }),
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
        menu::init(cx);

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
            key("secondary-alt-p", ToggleFrameStats, cx),
            key("secondary-[", GoBack, cx),
            key("secondary-]", GoForward, cx),
            key("secondary-k", ShowCommands, cx),
            KeyBinding::new("i", OpenBinary, Some(empty::CONTEXT)),
            KeyBinding::new("g", GoTo, Some(WORKSPACE)),
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

        // Open the first window through the same entry point secondary windows use.
        window::open(cx, None).expect("failed to open window");

        #[cfg(target_os = "macos")]
        logo::set_dock_icon();

        // Bring the app to the front.
        cx.activate(true);
    });
}
