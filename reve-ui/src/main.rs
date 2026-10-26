use gpui_kit::assets::IconName;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, Theme, TitleBar, h_flex, v_flex,
};
use gpui_kit::*;

// A "Quit" command the keyboard shortcut and the menu can both trigger.
actions!(reve, [Quit]);

// The main view: header, body, footer stacked top to bottom.
struct Reve;

impl Render for Reve {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        // Sidebar: Project above Symbol Tree. Keeps its width when the window resizes.
        let sidebar = resizable_panel()
            .size(px(264.))
            .size_range(px(200.)..px(480.))
            .fixed()
            .child(
                v_resizable("sidebar")
                    .child(resizable_panel().child(pane(
                        theme,
                        IconName::Folder,
                        theme.cyan,
                        "Project",
                    )))
                    .child(resizable_panel().child(pane(
                        theme,
                        IconName::ListTree,
                        theme.magenta,
                        "Symbol Tree",
                    ))),
            );

        // Code: Listing beside Decompile. They share the leftover width.
        let code = h_resizable("code")
            .child(
                resizable_panel()
                    .child(pane(theme, IconName::Cpu, theme.blue, "Listing").bg(theme.background)),
            )
            .child(resizable_panel().child(
                pane(theme, IconName::Braces, theme.green, "Decompile").bg(theme.background),
            ));

        // Bottom panel under the code. Keeps its height when the window resizes.
        let bottom = resizable_panel()
            .size(px(210.))
            .size_range(px(120.)..px(480.))
            .fixed()
            .child(
                pane(theme, IconName::SquareFunction, theme.magenta, "Functions")
                    .bg(theme.background),
            );

        v_flex()
            .size_full()
            .bg(theme.title_bar)
            // Header: drags the window, holds the traffic lights. Empty for now.
            .child(TitleBar::new().bg(transparent_black()).border_b_0())
            // Body: sidebar | code over bottom panel. Dividers can be dragged.
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("body").child(sidebar).child(
                        resizable_panel().child(
                            v_resizable("main")
                                .child(resizable_panel().child(code))
                                .child(bottom),
                        ),
                    ),
                ),
            )
            // Footer: the status line will go here.
            .child(div().h(px(28.)))
    }
}

// One pane: a header row (colored icon + title) over an empty body.
fn pane(theme: &Theme, icon: IconName, hue: Hsla, title: &'static str) -> Div {
    v_flex().size_full().child(
        h_flex()
            .h(px(32.))
            .flex_none()
            .px_3()
            .gap_2()
            .text_xs()
            .font_medium()
            .text_color(theme.muted_foreground)
            .child(Icon::new(icon).xsmall().text_color(hue))
            .child(title),
    )
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            // Set up gpui-kit (theme, components). Must run first.
            gpui_kit::init(cx);

            // Start in the system's light or dark mode.
            Theme::sync_system_appearance(None, cx);

            // Quit on Cmd+Q (Ctrl+Q on Linux/Windows) or from the app menu.
            cx.on_action(|_: &Quit, cx| cx.quit());
            let quit = if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            };
            cx.bind_keys([KeyBinding::new(quit, Quit, None)]);
            cx.set_menus([Menu::new("rêve").items([MenuItem::action("Quit rêve", Quit)])]);

            // Also quit when the last window is closed.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Open a 1440x900 window in the middle of the screen.
            // The system title bar is hidden; our header draws in its place.
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
                window_min_size: Some(size(px(1024.), px(600.))),
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
                cx.new(|_| Reve)
            })
            .expect("failed to open window");

            // Bring the app to the front.
            cx.activate(true);
        });
}
