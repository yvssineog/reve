use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Placement, TitleBar, h_flex, oklch,
};
use gpui_kit::*;

use crate::{GoBack, GoForward, GoTo, ToggleAppearance, ToggleBottom, ToggleSidebar, icon};

pub const HEIGHT: Pixels = px(42.);
// Header glyphs are 18px Phosphor icons.
const GLYPH: Pixels = px(18.);

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
        tool("sidebar", panel(sidebar_open), cx),
        if sidebar_open {
            "Hide Project and Symbol Tree"
        } else {
            "Show Project and Symbol Tree"
        },
        ToggleSidebar,
    );
    // The sidebar glyph turned a quarter counterclockwise, so its panel sits at the bottom.
    let bottom = command(
        tool("bottom", panel(bottom_open).rotate(radians(-FRAC_PI_2)), cx),
        if bottom_open {
            "Hide windows"
        } else {
            "Show windows"
        },
        ToggleBottom,
    );
    // A yellow moon in the dark, a purple sun in the light.
    let (glyph, hue) = if dark {
        ("moon-duotone", oklch(0.86, 0.15, 88.))
    } else {
        ("sun-duotone", oklch(0.56, 0.17, 298.4))
    };
    let appearance = command(
        tool("appearance", icon(glyph, GLYPH).text_color(hue), cx),
        if dark {
            "Switch to light mode"
        } else {
            "Switch to dark mode"
        },
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
                    .ml(px(78.))
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

// The panel toggle glyph: a rounded square with the panel cut out, wider when open.
fn panel(open: bool) -> Icon {
    icon(if open { "panel-open" } else { "panel-closed" }, GLYPH)
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

// The badge naming a control's one-key shortcut.
pub fn key_badge(key: &'static str, cx: &App) -> Div {
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
