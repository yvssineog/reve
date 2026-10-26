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
    /// Functions discovered or imported by the analysis engine.
    pub functions: Vec<Function>,
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
}
