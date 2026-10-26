use std::ops::Range;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::*;
use reve_model::{Address, Decompilation, DecompilationStatus};

use super::listing::{GUTTER, Listing, ROW};
use super::note;
use crate::palette::Palette;

// The function the listing cursor is in, as Ghidra decompiled it.
pub struct Decompile {
    listing: Entity<Listing>,
    // The shown function's entry.
    entry: Address,
    name: SharedString,
    // Its C, one line each, or why there is none.
    code: Result<Vec<SharedString>, SharedString>,
    scroll: UniformListScrollHandle,
    _sync: Subscription,
}

impl Decompile {
    // Opens on the listing cursor's function and follows the cursor from there.
    pub fn new(listing: Entity<Listing>, cx: &mut Context<Self>) -> Self {
        let sync = cx.observe(&listing, |this, _, cx| this.sync(cx));
        let function = listing.read(cx).function();
        Self {
            entry: function.entry,
            name: function.name.clone().into(),
            code: code(function.decompilation.as_ref()),
            listing,
            scroll: UniformListScrollHandle::new(),
            _sync: sync,
        }
    }

    // The function shown, for the pane header.
    pub fn name(&self) -> SharedString {
        self.name.clone()
    }

    // Shows the listing cursor's function, opened at the top, once the cursor leaves the
    // one shown.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let function = self.listing.read(cx).function();
        if function.entry == self.entry {
            return;
        }
        self.entry = function.entry;
        self.name = function.name.clone().into();
        self.code = code(function.decompilation.as_ref());
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(Point::default());
        cx.notify();
    }
}

impl Render for Decompile {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = match &self.code {
            Ok(lines) => lines,
            Err(why) => return note(why.clone(), cx).into_any_element(),
        };
        // The list is as wide as its longest line, so it scrolls sideways to that line's end.
        let longest = (0..lines.len()).max_by_key(|&ix| lines[ix].len());

        div()
            .flex_1()
            .min_h_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.))
            .line_height(ROW)
            .whitespace_nowrap()
            .text_color(Palette::of(cx).fg)
            .child(
                uniform_list(
                    "decompile",
                    lines.len(),
                    cx.processor(|this, range: Range<usize>, _, _| {
                        let lines = this.code.as_deref().unwrap_or_default();
                        lines[range]
                            .iter()
                            .map(|line| div().h(ROW).px(GUTTER).child(line.clone()))
                            .map(IntoElement::into_any_element)
                            .collect()
                    }),
                )
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(longest)
                .track_scroll(&self.scroll)
                .size_full()
                .pt_1()
                .pb_10(),
            )
            .scrollbar(&self.scroll, ScrollbarAxis::Both)
            .into_any_element()
    }
}

// A function's C as lines, without the blank lines Ghidra pads it with, or why there is none.
fn code(decompilation: Option<&Decompilation>) -> Result<Vec<SharedString>, SharedString> {
    let Some(decompilation) = decompilation else {
        return Err("No decompilation.".into());
    };
    let c = decompilation.c_code.trim();
    let diagnostic = |fallback: &str| {
        let message = decompilation.error_message.as_deref().unwrap_or(fallback);
        message.to_owned().into()
    };
    match decompilation.status {
        DecompilationStatus::Succeeded if !c.is_empty() => {
            Ok(c.lines().map(|line| line.to_owned().into()).collect())
        }
        DecompilationStatus::Failed => Err(diagnostic("Ghidra couldn't decompile this function.")),
        DecompilationStatus::TimedOut => Err(diagnostic("Decompilation timed out.")),
        DecompilationStatus::NotAttempted => Err("Not decompiled.".into()),
        _ => Err("No decompilation.".into()),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;
    use reve_model::{Decompilation, DecompilationStatus};

    use super::code;

    fn decompilation(
        status: DecompilationStatus,
        c_code: &str,
        error_message: Option<&str>,
    ) -> Decompilation {
        Decompilation {
            status,
            c_code: c_code.into(),
            error_message: error_message.map(Into::into),
            ..Decompilation::default()
        }
    }

    #[test]
    fn statuses_map_to_code_or_reason() {
        let mapped = |status, c_code, error_message| {
            code(Some(&decompilation(status, c_code, error_message)))
        };
        let reason = |text: &str| Err(SharedString::from(text.to_owned()));
        assert_eq!(
            mapped(
                DecompilationStatus::Succeeded,
                "\n\nint f(void)\n{\n}\n\n",
                None
            ),
            Ok(vec!["int f(void)".into(), "{".into(), "}".into()]),
        );
        assert_eq!(
            mapped(DecompilationStatus::Failed, "", Some("Low-level Error")),
            reason("Low-level Error"),
        );
        assert_eq!(
            mapped(DecompilationStatus::TimedOut, "", None),
            reason("Decompilation timed out."),
        );
        assert_eq!(code(None), reason("No decompilation."));
        assert_eq!(
            mapped(DecompilationStatus::Succeeded, " \n\n ", None),
            reason("No decompilation."),
            "blank code",
        );
    }
}
