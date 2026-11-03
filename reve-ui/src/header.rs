use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Placement, TitleBar, h_flex, oklch,
};
use gpui_kit::*;

use crate::recent::RecentStore;
use crate::{
    ClearRecent, CloseBinary, CloseWindow, GoBack, GoForward, GoTo, OpenBinary,
    OpenBinaryInNewWindow, OpenRecent, OpenRecentBinary, RemoveRecentBinary, ToggleAppearance,
    ToggleBottom, ToggleSidebar, icon,
};

pub const HEIGHT: Pixels = px(42.);
// Header glyphs are 18px Phosphor icons.
const GLYPH: Pixels = px(18.);

// macOS places traffic lights at the leading edge; other platforms have none there.
pub fn file_menu_left_inset() -> Pixels {
    if cfg!(target_os = "macos") {
        px(78.)
    } else {
        px(0.)
    }
}

// The window's header, which panels it shows open, and whether Back and Forward have
// anywhere to go. Notifying it redraws the window too.
pub struct Header {
    pub sidebar_open: bool,
    pub bottom_open: bool,
    pub back: bool,
    pub forward: bool,
}

impl Render for Header {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        header(self, window, cx)
    }
}

// Left: sidebar toggle and history. Center: the "Go to" field. Right: light/dark
// switch and windows toggle.
// Both sides take half the leftover width, so the field sits on the window's center.
fn header(header: &Header, window: &mut Window, cx: &mut App) -> TitleBar {
    let &Header {
        sidebar_open,
        bottom_open,
        back,
        forward,
    } = header;
    let dark = cx.theme().mode.is_dark();
    let sidebar = command(
        tool("sidebar", sidebar_glyph(sidebar_open, GLYPH), cx),
        sidebar_label(sidebar_open),
        ToggleSidebar,
    );
    let bottom = command(
        tool("bottom", bottom_glyph(bottom_open, GLYPH), cx),
        bottom_label(bottom_open),
        ToggleBottom,
    );
    let appearance = command(
        tool("appearance", appearance_glyph(dark, GLYPH), cx),
        appearance_label(dark),
        ToggleAppearance,
    );
    let back = command(tool("back", icon("caret-left", GLYPH), cx), "Back", GoBack).disabled(!back);
    let forward = command(
        tool("forward", icon("caret-right", GLYPH), cx),
        "Forward",
        GoForward,
    )
    .disabled(!forward);

    TitleBar::new()
        .h(HEIGHT)
        .px_1p5()
        .child(
            h_flex().flex_1().child(
                controls()
                    .ml(file_menu_left_inset())
                    .child(file_menu(cx))
                    .child(sidebar)
                    .child(back)
                    .child(forward),
            ),
        )
        .child(controls().child(go_to(window, cx)))
        .child(
            h_flex()
                .flex_1()
                .justify_end()
                .child(controls().child(appearance).child(bottom)),
        )
}

// An in-window File menu because Windows does not show the app's native menu
// alongside rêve's custom title bar.
pub fn file_menu(cx: &App) -> impl IntoElement {
    let ink = cx.theme().foreground;
    Button::new("file-menu")
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(ink.opacity(0.64))
                .hover(ink.opacity(0.08))
                .active(ink.opacity(0.12)),
        )
        .h_7()
        .px_2()
        .rounded(px(6.))
        .text_size(px(12.5))
        .child("File")
        .dropdown_menu(move |mut menu, window, cx| {
            menu = menu
                .menu("Open…", Box::new(OpenBinary))
                .menu("Open in New Window…", Box::new(OpenBinaryInNewWindow));

            let files = cx.global::<RecentStore>().entries();
            if files.is_empty() {
                menu =
                    menu.separator()
                        .menu_with_disabled("Open Recent", Box::new(OpenRecent), true);
            } else {
                menu = menu.separator().submenu(
                    "Open Recent",
                    window,
                    cx,
                    |mut recent, window, cx| {
                        for file in cx.global::<RecentStore>().entries() {
                            let path = file.path;
                            let name = format!(
                                "{} — {}",
                                file.name,
                                path.parent()
                                    .map(|parent| parent.display().to_string())
                                    .unwrap_or_default()
                            );
                            let available = file.available;
                            recent =
                                recent.submenu(name, window, cx, move |mut file_menu, _, _| {
                                    if available {
                                        file_menu = file_menu
                                            .menu(
                                                "Open in This Window",
                                                Box::new(OpenRecentBinary {
                                                    path: path.clone(),
                                                    in_new_window: false,
                                                }),
                                            )
                                            .menu(
                                                "Open in New Window",
                                                Box::new(OpenRecentBinary {
                                                    path: path.clone(),
                                                    in_new_window: true,
                                                }),
                                            )
                                            .separator();
                                    } else {
                                        file_menu = file_menu.label("File is missing").separator();
                                    }
                                    file_menu.menu(
                                        "Remove from Recent",
                                        Box::new(RemoveRecentBinary { path: path.clone() }),
                                    )
                                });
                        }
                        recent
                            .separator()
                            .menu("Clear Recent", Box::new(ClearRecent))
                    },
                );
            }

            menu.separator()
                .menu("Close Binary", Box::new(CloseBinary))
                .menu("Close Window", Box::new(CloseWindow))
        })
}

// What the toggles do from where they stand, worded alike in tooltips and the command palette.
pub fn sidebar_label(open: bool) -> &'static str {
    if open {
        "Hide Project and Symbol Tree"
    } else {
        "Show Project and Symbol Tree"
    }
}

pub fn bottom_label(open: bool) -> &'static str {
    if open { "Hide windows" } else { "Show windows" }
}

pub fn appearance_label(dark: bool) -> &'static str {
    if dark {
        "Switch to light mode"
    } else {
        "Switch to dark mode"
    }
}

// A 28px icon button: dim at rest, a faint backdrop under the pointer.
// No hover animation; the change lands on the frame the pointer arrives.
fn tool(id: &'static str, icon: impl IntoElement, cx: &App) -> Button {
    let ink = cx.theme().foreground;
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(ink.opacity(0.64))
                .hover(ink.opacity(0.08))
                .active(ink.opacity(0.12)),
        )
        .size_7()
        .p_0()
        .rounded(px(6.))
        .child(icon)
}

// A button that runs `action`. Its tooltip shows the action's shortcut.
fn command(button: Button, label: &'static str, action: impl Action + Clone) -> Button {
    button
        .tooltip_with_action(label, &action, None)
        .tooltip_placement(Placement::Bottom)
        .tooltip_show_delay(Duration::from_millis(400))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

// The toggles' glyphs, alike here and in the command palette. The sidebar's is a rounded
// square with the panel cut out, wider when open.
pub fn sidebar_glyph(open: bool, size: Pixels) -> Icon {
    icon(if open { "panel-open" } else { "panel-closed" }, size)
}

// The sidebar's glyph turned a quarter counterclockwise, so its panel sits at the bottom.
pub fn bottom_glyph(open: bool, size: Pixels) -> Icon {
    sidebar_glyph(open, size).rotate(radians(-FRAC_PI_2))
}

// A yellow moon in the dark, a purple sun in the light.
pub fn appearance_glyph(dark: bool, size: Pixels) -> Icon {
    let (glyph, hue) = if dark {
        ("moon-duotone", oklch(0.86, 0.15, 88.))
    } else {
        ("sun-duotone", oklch(0.56, 0.17, 298.4))
    };
    icon(glyph, size).text_color(hue)
}

// The centered "Go to address or symbol" field. Its corners are concentric
// with the shortcut badge's: 5px badge radius plus 5px inset.
fn go_to(window: &Window, cx: &App) -> Button {
    let ink = cx.theme().foreground;
    Button::new("go-to")
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(ink.opacity(0.56))
                .hover(ink.opacity(0.14))
                .active(ink.opacity(0.18)),
        )
        // Set here rather than as the variant's color, which the kit paints at 20%.
        .bg(ink.opacity(0.1))
        .h_7()
        .w(px(360.).min(window.viewport_size().width * 0.32))
        .p_0()
        .rounded(px(10.))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .pl(px(9.))
                .pr(px(5.))
                .text_size(px(12.5))
                .child(icon("magnifying-glass-bold", px(14.)))
                .child("Go to address or symbol")
                .child(key_badge("G", cx).ml_auto()),
        )
        .on_click(|_, window, cx| window.dispatch_action(GoTo.boxed_clone(), cx))
}

// The badge naming a control's shortcut.
pub fn key_badge(key: impl IntoElement, cx: &App) -> Div {
    let ink = cx.theme().foreground;
    h_flex()
        .h(px(18.))
        .min_w(px(18.))
        .px_1()
        .justify_center()
        .rounded(px(5.))
        .bg(ink.opacity(0.08))
        .text_size(px(11.))
        .text_color(ink.opacity(0.64))
        .child(key)
}

// A group of header controls. It keeps mouse presses to itself, so clicking
// a control never drags the window or, on double click, zooms it.
fn controls() -> Div {
    h_flex()
        .gap_1()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}
