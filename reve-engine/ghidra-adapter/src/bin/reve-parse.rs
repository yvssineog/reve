//! Small command-line demo for the Ghidra adapter.

use std::{
    error::Error,
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use reve_ghidra_adapter::{GhidraAdapter, GhidraConfig};
use reve_model::{DecompilationStatus, Program};

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("reve-parse: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    let input = parse_input_argument(arguments)?;
    let config = GhidraConfig::from_environment();
    let program = GhidraAdapter::new(config).analyze(input)?;
    write_summary(&program, &mut io::stdout())?;
    Ok(())
}

fn parse_input_argument(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<PathBuf, CliError> {
    let mut arguments = arguments.into_iter();
    let Some(input) = arguments.next() else {
        return Err(CliError::Usage);
    };
    if arguments.next().is_some() {
        return Err(CliError::Usage);
    }
    Ok(PathBuf::from(input))
}

fn write_summary(program: &Program, mut output: impl Write) -> io::Result<()> {
    writeln!(output, "Program: {}", program.name)?;
    writeln!(output, "Format: {}", program.format)?;
    writeln!(
        output,
        "Architecture: {} ({}-bit addresses)",
        program.architecture, program.address_size_bits
    )?;
    writeln!(output, "Functions: {}", program.functions.len())?;
    let functions_with_pseudocode = program
        .functions
        .iter()
        .filter(|function| {
            function
                .decompilation
                .as_ref()
                .is_some_and(|decompilation| {
                    decompilation.status == DecompilationStatus::Succeeded
                        && !decompilation.c_code.trim().is_empty()
                })
        })
        .count();
    writeln!(
        output,
        "Functions with C pseudocode: {functions_with_pseudocode}/{}",
        program.functions.len()
    )?;

    const DISPLAY_LIMIT: usize = 10;
    for function in program.functions.iter().take(DISPLAY_LIMIT) {
        writeln!(
            output,
            "  0x{:X}  {} ({} instructions, {} bytes)",
            function.entry.0,
            function.name,
            function.instructions.len(),
            function.size
        )?;
    }

    if program.functions.len() > DISPLAY_LIMIT {
        writeln!(
            output,
            "  ... and {} more",
            program.functions.len() - DISPLAY_LIMIT
        )?;
    }
    Ok(())
}

#[derive(Debug)]
enum CliError {
    Usage,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage => write!(formatter, "usage: reve-parse <path-to-elf>"),
        }
    }
}

impl Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;
    use reve_model::{Address, Function};

    #[test]
    fn cli_requires_exactly_one_input_path() {
        assert!(matches!(
            parse_input_argument(Vec::<OsString>::new()),
            Err(CliError::Usage)
        ));
        assert!(matches!(
            parse_input_argument(["one.elf".into(), "two.elf".into()]),
            Err(CliError::Usage)
        ));
        assert_eq!(
            parse_input_argument(["input with spaces.elf".into()]).unwrap(),
            PathBuf::from("input with spaces.elf")
        );
    }

    #[test]
    fn summary_displays_program_and_caps_function_list() {
        let program = Program {
            name: "sample.elf".into(),
            format: "ELF".into(),
            architecture: "x86:LE:64:default".into(),
            address_size_bits: 64,
            functions: (0..12)
                .map(|index| Function {
                    name: format!("fn_{index}"),
                    entry: Address(index),
                    size: 3,
                    instructions: Vec::new(),
                    decompilation: None,
                })
                .collect(),
        };
        let mut summary = Vec::new();

        write_summary(&program, &mut summary).unwrap();

        let summary = String::from_utf8(summary).unwrap();
        assert!(summary.contains("Program: sample.elf"));
        assert!(summary.contains("Functions: 12"));
        assert!(summary.contains("Functions with C pseudocode: 0/12"));
        assert!(summary.contains("fn_9"));
        assert!(!summary.contains("fn_10"));
        assert!(summary.contains("... and 2 more"));
    }

    #[test]
    fn summary_counts_only_successful_nonempty_pseudocode() {
        use reve_model::{Decompilation, Function};

        let function = |name: &str, decompilation| Function {
            name: name.into(),
            entry: Address(0),
            size: 0,
            instructions: Vec::new(),
            decompilation,
        };
        let program = Program {
            name: "sample.elf".into(),
            format: "ELF".into(),
            architecture: "x86:LE:64:default".into(),
            address_size_bits: 64,
            functions: vec![
                function(
                    "success",
                    Some(Decompilation {
                        status: DecompilationStatus::Succeeded,
                        signature: "void success(void)".into(),
                        c_code: "void success(void) {}".into(),
                        error_message: None,
                    }),
                ),
                function(
                    "failed",
                    Some(Decompilation {
                        status: DecompilationStatus::Failed,
                        signature: String::new(),
                        c_code: String::new(),
                        error_message: Some("failed".into()),
                    }),
                ),
                function(
                    "empty-success",
                    Some(Decompilation {
                        status: DecompilationStatus::Succeeded,
                        signature: "void empty_success(void)".into(),
                        c_code: "  \n".into(),
                        error_message: None,
                    }),
                ),
                function("absent", None),
            ],
        };
        let mut summary = Vec::new();

        write_summary(&program, &mut summary).unwrap();

        let summary = String::from_utf8(summary).unwrap();
        assert!(summary.contains("Functions with C pseudocode: 1/4"));
    }
}
