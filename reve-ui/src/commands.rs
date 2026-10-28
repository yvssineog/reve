use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{WindowExt as _, h_flex};
use gpui_kit::*;

// A command the palette lists: what it reads as, and the action it runs.
pub type Entry = (&'static str, Box<dyn Action>);

// Opens the command palette: `commands` with their shortcuts, narrowed by typing. Picking one
// closes the palette before running it, so it runs where the keyboard was and reaches the
// screen or pane that handles it.
pub fn open(commands: Vec<Entry>, window: &mut Window, cx: &mut App) {
    // Shortcuts as they apply where the keyboard is, single letters bound to a pane included.
    let focused = window.focused(cx);
    let keys: Vec<Option<Kbd>> = commands
        .iter()
        .map(|(_, action)| {
            let focused = focused.as_ref()?;
            Kbd::binding_for_action_in(action.as_ref(), focused, window)
        })
        .collect();
    let commands = Rc::new(commands);
    let query = cx.new(|cx| CommandState::new(window, cx));
    present(query.clone(), window, cx, move |_, _| {
        let items = commands.iter().zip(&keys).map(|(&(label, _), key)| {
            let key = key.clone();
            CommandItem::new().label(label).child(move |_, _| {
                h_flex()
                    .flex_1()
                    .justify_between()
                    .child(label)
                    .children(key.clone())
            })
        });
        let picked = commands.clone();
        Command::new(&query)
            .bordered(false)
            .placeholder("Search commands")
            .items(items)
            .on_confirm(move |ix, window, cx| {
                let action = picked[ix.row].1.boxed_clone();
                window.close_dialog(cx);
                window.dispatch_action(action, cx);
            })
    });
}

// Shows `content`, a palette searching with `query`, in a dialog below the header. The dialog
// takes focus as it opens; the query takes it once, not on every rebuild.
pub fn present<E: IntoElement>(
    query: Entity<CommandState>,
    window: &mut Window,
    cx: &mut App,
    content: impl Fn(&mut Window, &mut App) -> E + 'static,
) {
    let opening = Cell::new(true);
    window.open_dialog(cx, move |dialog, window, cx| {
        if opening.replace(false) {
            query.update(cx, |query, cx| query.focus(window, cx));
        }
        dialog
            .w(px(440.))
            .margin_top(px(72.))
            .p_0()
            .min_h_0()
            .close_button(false)
            .entrance(DialogEntrance::None)
            .child(content(window, cx))
    });
}
