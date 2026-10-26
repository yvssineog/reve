use gpui_kit::*;

// A "Quit" command the keyboard shortcut and the menu can both trigger.
actions!(reve, [Quit]);

// The main view. Empty for now, it just fills the window.
struct Reve;

impl Render for Reve {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            // Set up gpui-kit (theme, components). Must run first.
            gpui_kit::init(cx);

            // Quit on Cmd+Q (Ctrl+Q on Linux/Windows) or from the app menu.
            cx.on_action(|_: &Quit, cx| cx.quit());
            let quit = if cfg!(target_os = "macos") { "cmd-q" } else { "ctrl-q" };
            cx.bind_keys([KeyBinding::new(quit, Quit, None)]);
            cx.set_menus([Menu::new("rêve").items([MenuItem::action("Quit rêve", Quit)])]);

            // Also quit when the last window is closed.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Open a 1280x800 window in the middle of the screen, titled "rêve".
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
                titlebar: Some(TitlebarOptions {
                    title: Some("rêve".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Reve))
                .expect("failed to open window");

            // Bring the app to the front.
            cx.activate(true);
        });
}
