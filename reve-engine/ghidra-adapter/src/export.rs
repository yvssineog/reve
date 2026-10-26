//! Read and convert the binary Protocol Buffers export written by `Export.java`.

use std::{fs, io, path::Path};

use prost::Message;
use reve_model::{Address, Function, Instruction, Program};

use crate::proto::reve::engine::v1 as wire;

/// Errors encountered while reading or decoding a Ghidra export.
#[derive(Debug)]
pub enum ExportError {
    /// The export file could not be read from disk.
    Io(io::Error),
    /// The file did not contain a valid `reve.engine.v1.Program` message.
    Protobuf(prost::DecodeError),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read protobuf export: {error}"),
            Self::Protobuf(error) => write!(formatter, "invalid protobuf export: {error}"),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Protobuf(error) => Some(error),
        }
    }
}

impl From<io::Error> for ExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<prost::DecodeError> for ExportError {
    fn from(error: prost::DecodeError) -> Self {
        Self::Protobuf(error)
    }
}

/// Read a binary protobuf `Program` file and convert it to `reve_model::Program`.
pub fn read_export(path: impl AsRef<Path>) -> Result<Program, ExportError> {
    let bytes = fs::read(path)?;
    let message = wire::Program::decode(bytes.as_slice())?;
    Ok(into_model(message))
}

/// Convert generated protobuf wire types into the model shared with the UI.
pub fn into_model(message: wire::Program) -> Program {
    Program {
        name: message.name,
        format: message.format,
        architecture: message.architecture,
        address_size_bits: message.address_size_bits,
        functions: message
            .functions
            .into_iter()
            .map(function_into_model)
            .collect(),
    }
}

fn function_into_model(message: wire::Function) -> Function {
    Function {
        name: message.name,
        entry: Address(message.entry),
        size: message.size,
        instructions: message
            .instructions
            .into_iter()
            .map(instruction_into_model)
            .collect(),
    }
}

fn instruction_into_model(message: wire::Instruction) -> Instruction {
    Instruction {
        address: Address(message.address),
        size: message.size,
        mnemonic: message.mnemonic,
        operands: message.operands,
        bytes: message.bytes,
    }
}
