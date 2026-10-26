use super::{Function, Line, Syntax};
use crate::mock::DECOMPILED;

const KEYWORDS: &[&str] = &[
    "if", "else", "while", "for", "return", "break", "struct", "sizeof",
];
const TYPES: &[&str] = &["void", "int", "char", "uint", "bool", "byte", "FILE"];
const CONSTANTS: &[&str] = &["true", "false", "NULL"];

// TODO: the Ghidra adapter should hand over each function's lines, already marked, instead
// of this mock. A function reads as its plate comment, then its C.
pub fn functions() -> Vec<Function> {
    DECOMPILED
        .iter()
        .map(|&(name, note, code)| {
            let note = note.map(|note| format!("/* {note} */"));
            let lines = note.iter().map(String::as_str).chain(code.lines());
            Function {
                name,
                signature: usize::from(note.is_some()),
                lines: lines.map(lex).collect(),
            }
        })
        .collect()
}

// Marks a line the way Ghidra's decompiler does; what it leaves unmarked is plain.
fn lex(text: &str) -> Line {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut after_struct = false;
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let syntax = match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = text[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| at + 2 + end + 2);
                Some(Syntax::Comment)
            }
            quote @ (b'"' | b'\'') => {
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                at = (at + 1).min(bytes.len());
                Some(Syntax::String)
            }
            b'0'..=b'9' => {
                at += bytes[at..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric())
                    .count();
                Some(Syntax::Constant)
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                at += bytes[at..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_')
                    .count();
                let word = &text[start..at];
                let syntax = identifier(word, bytes.get(at) == Some(&b'('), after_struct);
                after_struct = word == "struct";
                syntax
            }
            _ => {
                at += 1;
                None
            }
        };
        spans.extend(syntax.map(|syntax| (start..at, syntax)));
    }
    Line {
        text: text.to_owned().into(),
        spans,
    }
}

// What an identifier names. A call names one of ours or a library's, even in capitals,
// as FUN_00413b10 does.
fn identifier(word: &str, called: bool, after_struct: bool) -> Option<Syntax> {
    if KEYWORDS.contains(&word) {
        Some(Syntax::Keyword)
    } else if CONSTANTS.contains(&word) {
        Some(Syntax::Constant)
    } else if called && DECOMPILED.iter().any(|&(name, ..)| name == word) {
        Some(Syntax::Function)
    } else if called {
        Some(Syntax::External)
    } else if TYPES.contains(&word) || word.ends_with("_t") || after_struct {
        Some(Syntax::Type)
    } else if word
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        Some(Syntax::Constant)
    } else {
        None
    }
}
