use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::actions::{Cancel, Confirm, SelectDown, SelectUp};
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::header::key_badge;
use crate::icon;
use crate::palette::Palette;

// The key context the menu binds its keys under.
const CONTEXT: &str = "CommandMenu";
// Row glyphs are 16px.
pub const GLYPH: Pixels = px(16.);

// Arrows move the highlight, enter picks it, escape clears the query and then closes. The
// field leaves these keys alone, so they reach the menu.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
    ]);
}

// Rows under a small heading.
pub struct Section {
    pub heading: &'static str,
    pub rows: Vec<Row>,
}

// Something to pick: a glyph, what it reads as, a quieter note at the right, and what
// picking it does.
#[derive(Clone)]
pub struct Row {
    pub icon: Icon,
    pub label: SharedString,
    pub note: Note,
    pub pick: Pick,
}

// What picking a row does.
pub type Pick = Rc<dyn Fn(&mut Window, &mut App)>;

// What a row says at its right.
#[derive(Clone)]
pub enum Note {
    None,
    Text(SharedString),
    // A shortcut, in the key badge's look.
    Key(Box<Kbd>),
}

// Turns a query into the sections it finds.
type Search = Box<dyn Fn(&str, &App) -> Vec<Section>>;

// A search field over the sections its query finds, one row highlighted.
struct CommandMenu {
    query: Entity<InputState>,
    search: Search,
    // What the query found, without sections it left empty.
    sections: Vec<Section>,
    // The highlighted row, counted across sections.
    selected: usize,
    // Said when a query finds nothing.
    empty: &'static str,
    // The rows' font.
    font: SharedString,
    _query: Subscription,
}

// Opens a menu below the header, its field focused. `search` turns the query into sections
// and runs again on every change.
pub fn open(
    placeholder: &'static str,
    empty: &'static str,
    font: SharedString,
    search: impl Fn(&str, &App) -> Vec<Section> + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let query = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    let menu = cx.new(|cx| {
        let changes = cx.subscribe(&query, |menu: &mut CommandMenu, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                menu.search(cx);
            }
        });
        let mut menu = CommandMenu {
            query: query.clone(),
            search: Box::new(search),
            sections: Vec::new(),
            selected: 0,
            empty,
            font,
            _query: changes,
        };
        menu.search(cx);
        menu
    });
    // The dialog takes focus as it opens; the field takes it once, not on every rebuild.
    let opening = Cell::new(true);
    window.open_dialog(cx, move |dialog, window, cx| {
        if opening.replace(false) {
            query.update(cx, |query, cx| query.focus(window, cx));
        }
        dialog
            .w(px(600.))
            .margin_top(px(72.))
            .p_0()
            .min_h_0()
            .rounded(px(12.))
            .close_button(false)
            .entrance(DialogEntrance::None)
            .child(menu.clone())
    });
}

impl CommandMenu {
    // Searches for what the field holds and highlights the first row.
    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value();
        let mut sections = (self.search)(query.trim(), cx);
        sections.retain(|section| !section.rows.is_empty());
        self.sections = sections;
        self.selected = 0;
        cx.notify();
    }

    // Every row, in the order shown.
    fn rows(&self) -> impl Iterator<Item = &Row> {
        self.sections.iter().flat_map(|section| &section.rows)
    }

    // Highlights the row at `ix`. The pointer can name a row from the frame before a search
    // that has since dropped it.
    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.rows().count() && self.selected != ix {
            self.selected = ix;
            cx.notify();
        }
    }

    // Moves the highlight by `step` rows, wrapping at either end.
    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let len = self.rows().count() as isize;
        if len > 0 {
            self.select((self.selected as isize + step).rem_euclid(len) as usize, cx);
        }
    }

    // Closes the menu, then runs the row at `ix`, so it acts where the keyboard was.
    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pick) = self.rows().nth(ix).map(|row| row.pick.clone()) else {
            return;
        };
        window.close_dialog(cx);
        pick(window, cx);
    }

    // Escape clears the query first; with nothing typed it goes on to the dialog, which closes.
    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.read(cx).value().is_empty() {
            cx.propagate();
            return;
        }
        // Setting the value raises no change event, so search here.
        self.query
            .update(cx, |query, cx| query.set_value("", window, cx));
        self.search(cx);
    }

    // A glyph, the label, then the note at the right; filled while highlighted.
    fn row(&self, row: &Row, ix: usize, palette: &Palette, cx: &Context<Self>) -> Stateful<Div> {
        let note = match &row.note {
            Note::None => None,
            Note::Text(text) => Some(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(palette.fg_3)
                    .child(text.clone()),
            ),
            Note::Key(key) => Some(key_badge(key.as_ref().clone().appearance(false), cx)),
        };
        h_flex()
            .id(ix)
            .h(px(32.))
            .px(px(10.))
            .gap(px(10.))
            .rounded(px(6.))
            .font_family(self.font.clone())
            .text_size(px(13.))
            .text_color(palette.fg)
            // The ink the header's buttons take under the pointer.
            .when(ix == self.selected, |row| {
                row.bg(cx.theme().foreground.opacity(0.08))
            })
            .on_mouse_move(cx.listener(move |menu, _, _, cx| menu.select(ix, cx)))
            .on_click(cx.listener(move |menu, _, window, cx| menu.pick(ix, window, cx)))
            .child(row.icon.clone())
            .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
            .children(note)
    }
}

impl Render for CommandMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::of(cx);
        let mut list = v_flex().p(px(6.));
        let mut ix = 0;
        for section in &self.sections {
            list = list.child(heading(section.heading, &palette));
            for row in &section.rows {
                list = list.child(self.row(row, ix, &palette, cx));
                ix += 1;
            }
        }
        // Nothing typed and nothing found leaves the field alone.
        let typed = !self.query.read(cx).value().trim().is_empty();
        let body = if ix > 0 {
            Some(list)
        } else if typed {
            Some(
                v_flex()
                    .py(px(20.))
                    .text_center()
                    .text_size(px(13.))
                    .text_color(palette.fg_3)
                    .child(self.empty),
            )
        } else {
            None
        };
        v_flex()
            .key_context(CONTEXT)
            .on_action(cx.listener(|menu, _: &SelectUp, _, cx| menu.step(-1, cx)))
            .on_action(cx.listener(|menu, _: &SelectDown, _, cx| menu.step(1, cx)))
            .on_action(
                cx.listener(|menu, _: &Confirm, window, cx| menu.pick(menu.selected, window, cx)),
            )
            .on_action(cx.listener(Self::cancel))
            .child(
                h_flex()
                    .h(px(52.))
                    .pl(px(16.))
                    .pr(px(12.))
                    .gap(px(12.))
                    .child(icon("magnifying-glass", px(18.)).text_color(palette.fg_3))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.query)
                                .appearance(false)
                                .p_0()
                                .text_size(px(15.)),
                        ),
                    )
                    .child(key_badge("Esc", cx)),
            )
            .children(body.map(|body| body.border_t_1().border_color(palette.hairline)))
    }
}

// A section's title, small and quiet, sitting on the bottom of its row.
fn heading(title: &'static str, palette: &Palette) -> Div {
    h_flex()
        .h(px(28.))
        .px(px(10.))
        .pb(px(6.))
        .items_end()
        .text_size(px(12.))
        .font_medium()
        .text_color(palette.fg_3)
        .child(title)
}
