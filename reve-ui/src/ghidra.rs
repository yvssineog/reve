use std::path::PathBuf;

use gpui_kit::*;
use reve_ghidra_adapter::{
    CancellationToken, EditError, GhidraAdapter, GhidraConfig, GhidraSession, ParserError,
};
use reve_model::{Address, Changes, Edit, Program};

use crate::panes::listing::Listing;

// Ghidra kept open on the binary's project to make the user's edits, one at a time, and what it
// is doing.
pub struct Ghidra {
    // Out on a background thread while it makes an edit; gone when it never started or was lost.
    session: Option<GhidraSession>,
    status: Status,
}

// What Ghidra is doing, as the footer says it.
pub enum Status {
    Starting,
    Ready,
    // The edit it is making.
    Editing(SharedString),
    // What the last edit did, a field each.
    Edited(Vec<SharedString>),
    // Why the last edit didn't happen, or why none can.
    Failed(SharedString),
}

impl Ghidra {
    // Opens the binary's project in Ghidra off the main thread. Cancelling stops it, and Ghidra
    // closes again if the entity is gone by the time it opens.
    pub fn start(binary: PathBuf, cancellation: CancellationToken, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let session = cx.background_spawn(async move {
                GhidraAdapter::new(GhidraConfig::from_environment()).session(binary, &cancellation)
            });
            cx.spawn(async move |this, cx| {
                let session = session.await;
                this.update(cx, |this: &mut Self, cx| this.started(session, cx))
                    .ok();
            })
            .detach();
            Self {
                session: None,
                status: Status::Starting,
            }
        })
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    fn started(&mut self, session: Result<GhidraSession, ParserError>, cx: &mut Context<Self>) {
        self.status = match session {
            Ok(session) => {
                self.session = Some(session);
                Status::Ready
            }
            Err(error) => {
                eprintln!("starting Ghidra for edits: {error}");
                Status::Failed(match error {
                    ParserError::GhidraFailed { status, .. } => {
                        format!("Ghidra couldn't open the binary for edits ({status})").into()
                    }
                    error => format!("Ghidra couldn't open the binary for edits: {error}").into(),
                })
            }
        };
        cx.notify();
    }

    // Whether Ghidra can take an edit now: it has opened the binary and isn't making another.
    pub fn ready(&self) -> bool {
        self.session.is_some()
    }

    // Has Ghidra make `edit` off the main thread, then shows what it changed in `listing`. Fails
    // at once, saying why, while Ghidra is starting, busy or gone; otherwise the status says how
    // it went. Reads nothing from `listing` yet, so the listing can ask for edits itself.
    pub fn edit(
        &mut self,
        edit: Edit,
        listing: Entity<Listing>,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        let Some(mut session) = self.session.take() else {
            return Err(match &self.status {
                Status::Starting => "Ghidra is still opening the binary.".into(),
                Status::Editing(_) => "Ghidra is still making the last edit.".into(),
                Status::Failed(why) => why.clone(),
                Status::Ready | Status::Edited(_) => "Ghidra can't make edits.".into(),
            });
        };
        self.status = Status::Editing(doing(&edit));
        cx.notify();
        let made = cx.background_spawn(async move {
            let changes = session.edit(&edit);
            (session, edit, changes)
        });
        cx.spawn(async move |this, cx| {
            let (session, edit, changes) = made.await;
            this.update(cx, |this, cx| {
                this.made(session, &edit, changes, &listing, cx)
            })
            .ok();
        })
        .detach();
        Ok(())
    }

    // Takes the session back and shows what the edit changed, or why it didn't. A lost
    // connection drops the session, which closes Ghidra.
    fn made(
        &mut self,
        session: GhidraSession,
        edit: &Edit,
        changes: Result<Changes, EditError>,
        listing: &Entity<Listing>,
        cx: &mut Context<Self>,
    ) {
        self.status = match changes {
            Ok(changes) => {
                self.session = Some(session);
                Status::Edited(listing.update(cx, |listing, cx| {
                    let said = said(edit, &changes, listing.program());
                    listing.apply(&changes, cx);
                    said
                }))
            }
            Err(EditError::Refused(why)) => {
                self.session = Some(session);
                Status::Failed(why.into())
            }
            Err(lost) => {
                eprintln!("editing in Ghidra: {lost}");
                Status::Failed("Lost Ghidra; reopen the binary to edit it.".into())
            }
        };
        cx.notify();
    }
}

// The name of the function at `entry`.
fn name(program: &Program, entry: Address) -> &str {
    program
        .functions
        .binary_search_by_key(&entry, |function| function.entry)
        .map_or("", |ix| &program.functions[ix].name)
}

// What making `edit` reads as.
fn doing(edit: &Edit) -> SharedString {
    match edit {
        Edit::Rename { name, .. } if name.is_empty() => "Giving back Ghidra's name".into(),
        Edit::Rename { name, .. } => format!("Renaming to {name}").into(),
        Edit::Comment { .. } => "Saving the comment".into(),
        Edit::Bookmark { .. } => "Saving the bookmark".into(),
    }
}

// What `edit` did to `program`, read before its changes go in: the edit, then how many
// functions Ghidra decompiled again.
fn said(edit: &Edit, changes: &Changes, program: &Program) -> Vec<SharedString> {
    let mut said: Vec<SharedString> = match edit {
        Edit::Rename { entry, .. } => {
            let now = changes
                .functions
                .iter()
                .find(|change| change.entry == *entry);
            let now = now.map_or("", |change| &change.name);
            vec![format!("Renamed {} to {now}", name(program, *entry)).into()]
        }
        Edit::Comment { text, .. } if text.is_empty() => vec!["Removed the comment".into()],
        Edit::Comment { .. } => vec!["Saved the comment".into()],
        Edit::Bookmark { on: true, .. } => vec!["Bookmarked".into()],
        Edit::Bookmark { on: false, .. } => vec!["Removed the bookmark".into()],
    };
    let decompiled = changes
        .functions
        .iter()
        .filter(|change| change.decompilation.is_some())
        .count();
    match decompiled {
        0 => {}
        1 => said.push("Decompiled 1 function again".into()),
        n => said.push(format!("Decompiled {n} functions again").into()),
    }
    said
}

#[cfg(test)]
mod tests {
    use reve_model::{Address, Changes, Decompilation, Edit, Function, FunctionChange, Program};

    use super::said;

    #[test]
    fn renames_say_both_names_and_how_much_was_decompiled_again() {
        let function = |name: &str, entry| Function {
            name: name.into(),
            entry: Address(entry),
            size: 0,
            instructions: Vec::new(),
            decompilation: None,
        };
        let program = Program {
            name: "demo".into(),
            format: "ELF".into(),
            architecture: "x86:LE:64:default".into(),
            address_size_bits: 64,
            functions: vec![function("main", 0x1000), function("FUN_00001010", 0x1010)],
            bookmarks: Vec::new(),
        };
        let change = |name: &str, entry| FunctionChange {
            entry: Address(entry),
            name: name.into(),
            decompilation: Some(Decompilation::default()),
        };
        let changes = Changes {
            functions: vec![change("parse", 0x1010), change("main", 0x1000)],
            ..Changes::default()
        };
        let edit = Edit::Rename {
            entry: Address(0x1010),
            name: "parse".into(),
        };
        assert_eq!(
            said(&edit, &changes, &program),
            [
                "Renamed FUN_00001010 to parse",
                "Decompiled 2 functions again"
            ]
        );
        let bookmark = Edit::Bookmark {
            address: Address(0x1000),
            on: true,
        };
        assert_eq!(
            said(&bookmark, &Changes::default(), &program),
            ["Bookmarked"]
        );
    }
}
