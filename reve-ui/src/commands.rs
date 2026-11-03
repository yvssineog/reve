use std::rc::Rc;

use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::*;

use crate::menu::{self, Note, Row, Section};

// A command. Picking it runs `action` where the keyboard was, so it reaches the screen or
// pane that handles it; its note is the action's shortcut there, single letters bound to a
// pane included.
pub fn row(
    icon: Icon,
    label: impl Into<SharedString>,
    action: Box<dyn Action>,
    window: &Window,
    cx: &App,
) -> Row {
    let key = window
        .focused(cx)
        .and_then(|focused| Kbd::binding_for_action_in(action.as_ref(), &focused, window));
    Row {
        icon,
        label: label.into(),
        note: key.map_or(Note::None, |key| Note::Key(Box::new(key))),
        pick: Rc::new(move |window, cx| window.dispatch_action(action.boxed_clone(), cx)),
    }
}

// Opens the command palette over `sections`, narrowed by typing.
pub fn open(sections: Vec<Section>, window: &mut Window, cx: &mut App) {
    let font = cx.theme().font_family.clone();
    menu::open(
        "Search commands",
        "No command by that name",
        font,
        move |query, _| matching(&sections, query),
        window,
        cx,
    );
}

// The commands whose labels hold `query`, ignoring case.
fn matching(sections: &[Section], query: &str) -> Vec<Section> {
    let query = query.to_lowercase();
    sections
        .iter()
        .map(|section| Section {
            heading: section.heading,
            rows: section
                .rows
                .iter()
                .filter(|row| row.label.to_lowercase().contains(&query))
                .cloned()
                .collect(),
        })
        .collect()
}
