use std::cell::Cell;

use gpui_kit::base::actions::Confirm;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;
use reve_model::{Address, Edit};

use crate::ghidra::{Ghidra, Status};
use crate::header::key_badge;
use crate::icon;
use crate::menu;
use crate::palette::Palette;
use crate::panes::listing::Listing;

// A field holding a function's name. Enter has Ghidra rename the function and decompile again
// every function that shows the name; the dialog closes once their C is in, or says why Ghidra
// refused.
struct Rename {
    field: Entity<InputState>,
    entry: Address,
    // The name it had when the dialog opened.
    name: String,
    listing: Entity<Listing>,
    ghidra: Entity<Ghidra>,
    state: State,
    _ghidra: Subscription,
}

enum State {
    Typing,
    Renaming,
    Refused(SharedString),
}

// Opens Rename below the header on the function the listing cursor names, its name selected.
// Cleared, the field shows the name Ghidra gives it back.
pub fn open(listing: Entity<Listing>, ghidra: Entity<Ghidra>, window: &mut Window, cx: &mut App) {
    let (entry, name) = {
        let function = listing.read(cx).named();
        (function.entry, function.name.clone())
    };
    let field = cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(name.clone())
            .placeholder(format!("FUN_{:08x}", entry.0))
    });
    let rename = cx.new(|cx| Rename {
        _ghidra: cx.observe_in(&ghidra, window, Rename::ghidra),
        field: field.clone(),
        entry,
        name,
        listing,
        ghidra,
        state: State::Typing,
    });
    // The field takes focus once, not on every rebuild, with the name selected to type over.
    let opening = Cell::new(true);
    window.open_dialog(cx, move |dialog, window, cx| {
        if opening.replace(false) {
            field.update(cx, |field, cx| {
                field.focus(window, cx);
                field.select_all(window, cx);
            });
        }
        dialog
            .w(px(480.))
            .margin_top(px(72.))
            .p_0()
            .min_h_0()
            .rounded(px(12.))
            .close_button(false)
            .entrance(DialogEntrance::None)
            .child(rename.clone())
    });
}

impl Rename {
    // Hands Ghidra the name on enter, which the dialog would otherwise take to close. Unchanged,
    // there is nothing to do, so the dialog closes.
    fn submit(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let State::Renaming = self.state {
            return;
        }
        let name = self.field.read(cx).value().trim().to_owned();
        if name == self.name {
            window.close_dialog(cx);
            return;
        }
        let edit = Edit::Rename {
            entry: self.entry,
            name,
        };
        let listing = self.listing.clone();
        self.state = match self
            .ghidra
            .update(cx, |ghidra, cx| ghidra.edit(edit, listing, cx))
        {
            Ok(()) => State::Renaming,
            Err(why) => State::Refused(why),
        };
        cx.notify();
    }

    // Closes once the rename is in, or shows why Ghidra refused it.
    fn ghidra(&mut self, ghidra: Entity<Ghidra>, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.state, State::Renaming) {
            return;
        }
        match ghidra.read(cx).status() {
            Status::Edited(_) => window.close_dialog(cx),
            Status::Failed(why) => {
                self.state = State::Refused(why.clone());
                cx.notify();
            }
            Status::Starting | Status::Ready | Status::Editing(_) => {}
        }
    }
}

impl Render for Rename {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::of(cx);
        let note = match &self.state {
            State::Typing => None,
            State::Renaming => Some(
                h_flex()
                    .gap_2()
                    .text_color(palette.fg_3)
                    .child(Spinner::new().with_size(px(12.)).color(palette.fg_3))
                    .child("Renaming in Ghidra and decompiling its callers again…"),
            ),
            State::Refused(why) => Some(h_flex().text_color(cx.theme().danger).child(why.clone())),
        };
        v_flex()
            .on_action(cx.listener(Self::submit))
            .child(
                // The command menu's field row: glyph, field, then Esc.
                h_flex()
                    .h(px(44.))
                    .px(px(16.))
                    .gap(px(10.))
                    .child(icon("function-duotone", menu::GLYPH).text_color(palette.symbol))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.field)
                                .appearance(false)
                                .p_0()
                                .text_size(px(14.)),
                        ),
                    )
                    .child(key_badge("Esc", cx)),
            )
            .children(note.map(|note| {
                note.min_h(px(36.))
                    .px(px(16.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(palette.hairline)
                    .text_size(px(12.))
            }))
    }
}
