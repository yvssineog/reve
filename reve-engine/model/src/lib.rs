//! Shared data types exchanged by the rêve engine and user interface.

/// An address in the analyzed program's address space.
///
/// This is distinct from a file offset: for ELF files it normally represents
/// the virtual address reported by Ghidra.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address(pub u64);

/// Metadata and analysis results for one loaded binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// Display name, usually the input file name.
    pub name: String,
    /// Container format, for example `ELF`.
    pub format: String,
    /// Processor/language identifier, for example `x86:LE:64:default`.
    pub architecture: String,
    /// Width of addresses in bits.
    pub address_size_bits: u32,
    /// Functions discovered or imported by the analysis engine, in entry order.
    pub functions: Vec<Function>,
    /// The user's bookmarks, ascending.
    pub bookmarks: Vec<Address>,
}

/// A function recognized by the analysis engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub name: String,
    pub entry: Address,
    /// Function size in bytes when known.
    pub size: u64,
    pub instructions: Vec<Instruction>,
    /// Decompilation outcome and pseudocode, when reported by the analysis engine.
    pub decompilation: Option<Decompilation>,
}

/// C-like pseudocode produced by the analysis engine for one function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Decompilation {
    /// Outcome of the decompilation attempt.
    pub status: DecompilationStatus,
    /// Decompiled function signature, when available.
    pub signature: String,
    /// C-like pseudocode; empty when decompilation did not succeed.
    pub c_code: String,
    /// Diagnostic from the decompiler on failure or timeout.
    pub error_message: Option<String>,
    /// The same pseudocode as the decompiler's markup, one entry per line.
    pub lines: Vec<Line>,
}

/// One line of decompiled C.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Line {
    /// The line's text in order, indentation first.
    pub tokens: Vec<Token>,
    /// Ascending addresses of the instructions this line's tokens came from.
    pub addresses: Vec<Address>,
}

/// A run of decompiled C and its syntax class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

/// The decompiler's syntax class for a token, the one Ghidra colors it by.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TokenKind {
    /// Punctuation, operators and whitespace.
    #[default]
    Plain,
    Keyword,
    Comment,
    Type,
    Function,
    Variable,
    Constant,
    Parameter,
    Global,
    Error,
    /// Decompiler annotations such as `<unknown>` or bit-range markers.
    Special,
}

/// Outcome of a function's decompilation attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DecompilationStatus {
    /// No recognized status was supplied, for example by an older or malformed export.
    #[default]
    Unspecified,
    /// The decompiler produced C-like pseudocode.
    Succeeded,
    /// The decompiler failed to produce pseudocode.
    Failed,
    /// The decompilation exceeded its time limit.
    TimedOut,
    /// Decompilation was deliberately skipped.
    NotAttempted,
}

/// A decoded machine instruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub address: Address,
    pub size: u32,
    pub mnemonic: String,
    /// Human-readable operands as rendered by the disassembler.
    pub operands: String,
    /// Original instruction bytes.
    pub bytes: Vec<u8>,
    /// End-of-line comment, when the instruction has one.
    pub comment: Option<String>,
}

/// A change the user asks the analysis engine to make to the program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Edit {
    /// Names the function at `entry`. An empty name restores the engine's default.
    Rename { entry: Address, name: String },
    /// Sets the end-of-line comment at the instruction at `address`. Empty text removes it.
    Comment { address: Address, text: String },
    /// Adds or removes the user's bookmark at `address`.
    Bookmark { address: Address, on: bool },
}

/// What an edit changed in the program.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Changes {
    /// Functions whose name or decompilation changed.
    pub functions: Vec<FunctionChange>,
    /// Instructions whose end-of-line comment changed, with the comment they have now.
    pub comments: Vec<(Address, Option<String>)>,
    /// Addresses now bookmarked, or no longer.
    pub bookmarks: Vec<(Address, bool)>,
}

/// A function's name and decompilation after an edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionChange {
    pub entry: Address,
    pub name: String,
    pub decompilation: Option<Decompilation>,
}

impl Program {
    /// Takes in what an edit changed. Changes to functions or instructions the program doesn't
    /// hold are left out.
    pub fn apply(&mut self, changes: &Changes) {
        for change in &changes.functions {
            if let Ok(ix) = self
                .functions
                .binary_search_by_key(&change.entry, |function| function.entry)
            {
                let function = &mut self.functions[ix];
                function.name.clone_from(&change.name);
                function.decompilation.clone_from(&change.decompilation);
            }
        }
        for (address, comment) in &changes.comments {
            let instruction = self
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.instructions)
                .find(|instruction| instruction.address == *address);
            if let Some(instruction) = instruction {
                instruction.comment.clone_from(comment);
            }
        }
        for &(address, on) in &changes.bookmarks {
            match (self.bookmarks.binary_search(&address), on) {
                (Err(ix), true) => self.bookmarks.insert(ix, address),
                (Ok(ix), false) => {
                    self.bookmarks.remove(ix);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(name: &str, entry: u64) -> Function {
        Function {
            name: name.into(),
            entry: Address(entry),
            size: 4,
            instructions: vec![Instruction {
                address: Address(entry),
                size: 4,
                mnemonic: "ret".into(),
                operands: String::new(),
                bytes: vec![0; 4],
                comment: None,
            }],
            decompilation: None,
        }
    }

    #[test]
    fn apply_takes_names_decompilations_comments_and_bookmarks() {
        let mut program = Program {
            name: "demo".into(),
            format: "ELF".into(),
            architecture: "x86:LE:64:default".into(),
            address_size_bits: 64,
            functions: vec![function("FUN_1000", 0x1000), function("FUN_2000", 0x2000)],
            bookmarks: vec![Address(0x1000)],
        };
        let decompilation = Decompilation {
            status: DecompilationStatus::Succeeded,
            signature: "void parse(void)".into(),
            ..Decompilation::default()
        };

        program.apply(&Changes {
            functions: vec![
                FunctionChange {
                    entry: Address(0x2000),
                    name: "parse".into(),
                    decompilation: Some(decompilation.clone()),
                },
                FunctionChange {
                    entry: Address(0x3000),
                    name: "elsewhere".into(),
                    decompilation: None,
                },
            ],
            comments: vec![(Address(0x1000), Some("checks the magic".into()))],
            bookmarks: vec![(Address(0x1000), false), (Address(0x2000), true)],
        });

        assert_eq!(program.functions[0].name, "FUN_1000");
        assert_eq!(program.functions[1].name, "parse");
        assert_eq!(program.functions[1].decompilation, Some(decompilation));
        assert_eq!(program.functions.len(), 2, "unknown functions stay out");
        assert_eq!(
            program.functions[0].instructions[0].comment.as_deref(),
            Some("checks the magic")
        );
        assert_eq!(program.bookmarks, [Address(0x2000)]);

        program.apply(&Changes {
            comments: vec![(Address(0x1000), None)],
            bookmarks: vec![(Address(0x2000), true)],
            ..Changes::default()
        });
        assert_eq!(program.functions[0].instructions[0].comment, None);
        assert_eq!(
            program.bookmarks,
            [Address(0x2000)],
            "no duplicate bookmarks"
        );
    }
}
