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
        decompilation: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{proto::reve::engine::v1 as wire, test_support::TempDirectory};
    use prost::Message;

    fn sample_message() -> wire::Program {
        wire::Program {
            name: "firmware.elf".into(),
            format: "ELF".into(),
            architecture: "x86:LE:64:default".into(),
            address_size_bits: 64,
            functions: vec![wire::Function {
                name: "entry".into(),
                entry: u64::MAX,
                size: 5,
                instructions: vec![wire::Instruction {
                    address: u64::MAX - 1,
                    size: 2,
                    mnemonic: "mov".into(),
                    operands: "rax, rbx".into(),
                    bytes: vec![0x48, 0x89],
                }],
                decompilation: None,
            }],
        }
    }

    #[test]
    fn into_model_preserves_unsigned_addresses_and_instruction_data() {
        let model = into_model(sample_message());

        assert_eq!(model.name, "firmware.elf");
        assert_eq!(model.format, "ELF");
        assert_eq!(model.architecture, "x86:LE:64:default");
        assert_eq!(model.address_size_bits, 64);
        assert_eq!(model.functions.len(), 1);
        assert_eq!(model.functions[0].entry, Address(u64::MAX));
        assert_eq!(model.functions[0].size, 5);
        assert_eq!(model.functions[0].instructions.len(), 1);
        assert_eq!(
            model.functions[0].instructions[0].address,
            Address(u64::MAX - 1)
        );
        assert_eq!(model.functions[0].instructions[0].bytes, [0x48, 0x89]);
    }

    #[test]
    fn read_export_decodes_a_valid_message_and_ignores_unknown_fields() {
        let temp = TempDirectory::new("export-valid").unwrap();
        let path = temp.path().join("result.pb");
        let mut bytes = sample_message().encode_to_vec();
        // Protobuf field 100, varint value 1. Older readers must skip it.
        bytes.extend_from_slice(&[0xA0, 0x06, 0x01]);
        fs::write(&path, bytes).unwrap();

        let model = read_export(&path).unwrap();
        assert_eq!(model.name, "firmware.elf");
        assert_eq!(model.functions[0].instructions[0].mnemonic, "mov");
    }

    #[test]
    fn read_export_reports_a_missing_file() {
        let temp = TempDirectory::new("export-missing").unwrap();

        assert!(matches!(
            read_export(temp.path().join("missing.pb")),
            Err(ExportError::Io(_))
        ));
    }

    #[test]
    fn read_export_reports_truncated_protobuf() {
        let temp = TempDirectory::new("export-malformed").unwrap();
        let path = temp.path().join("malformed.pb");
        fs::write(&path, [0x0A, 0x04, b'x']).unwrap();

        assert!(matches!(read_export(&path), Err(ExportError::Protobuf(_))));
    }

    #[test]
    fn empty_proto3_message_decodes_to_default_model() {
        let temp = TempDirectory::new("export-empty-message").unwrap();
        let path = temp.path().join("empty.pb");
        fs::write(&path, b"").unwrap();

        let model = read_export(&path).unwrap();
        assert!(model.name.is_empty());
        assert!(model.functions.is_empty());
    }
}
