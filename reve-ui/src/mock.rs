//! Stand-ins for the engine until the app loads binaries. `program` is shaped like the
//! `reve_model::Program` the Ghidra adapter exports. `DECOMPILED` is the one thing here the model
//! has no field for yet.

use reve_model::{Address, Function, Instruction, Program};

use crate::panes::listing::target;

// The analyzed httpd: the Electron build's functions, in address order.
pub fn program() -> Program {
    let functions = TEMPLATE.iter().map(|&(name, code)| {
        let instructions: Vec<Instruction> = code
            .iter()
            .map(|&(address, bytes, mnemonic, operands)| {
                let bytes: Vec<u8> = bytes
                    .split(' ')
                    .map(|byte| u8::from_str_radix(byte, 16).unwrap())
                    .collect();
                Instruction {
                    address: Address(address),
                    size: bytes.len() as u32,
                    mnemonic: mnemonic.into(),
                    operands: operands.into(),
                    bytes,
                }
            })
            .collect();
        Function {
            name: name.into(),
            entry: instructions[0].address,
            size: instructions
                .iter()
                .map(|instruction| u64::from(instruction.size))
                .sum(),
            instructions,
            decompilation: None,
        }
    });
    Program {
        name: "httpd".into(),
        format: "Executable and Linking Format (ELF)".into(),
        architecture: "ARM:LE:32:v7".into(),
        address_size_bits: 32,
        functions: functions.collect(),
    }
}

// Copies of the program, so the listing runs past a million rows.
const COPIES: u64 = 3600;
// Where each copy starts after the last, past the program's 0x4e34 bytes.
const SPAN: u64 = 0x10000;

// `program` COPIES times over, each copy a SPAN above the last, with the addresses its
// operands name moved up too.
pub fn stress(program: Program) -> Program {
    let functions = (0..COPIES)
        .flat_map(|copy| {
            program
                .functions
                .iter()
                .map(move |function| shifted(function, copy * SPAN))
        })
        .collect();
    Program {
        functions,
        ..program
    }
}

fn shifted(function: &Function, by: u64) -> Function {
    let instructions = function
        .instructions
        .iter()
        .map(|instruction| Instruction {
            address: Address(instruction.address.0 + by),
            size: instruction.size,
            mnemonic: instruction.mnemonic.clone(),
            operands: match target(&instruction.operands) {
                Some(address) => format!("{:#010x}", address.0 + by),
                None => instruction.operands.clone(),
            },
            bytes: instruction.bytes.clone(),
        })
        .collect();
    Function {
        name: function.name.clone(),
        entry: Address(function.entry.0 + by),
        size: function.size,
        instructions,
    }
}

// An instruction: address, bytes, mnemonic, operands as Ghidra writes them.
type Code = (u64, &'static str, &'static str, &'static str);

// The Electron build's listing, function by function.
#[rustfmt::skip]
const TEMPLATE: &[(&str, &[Code])] = &[
    ("main", &[
        (0x00410c40, "7d 05 c6 14", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00410c44, "1c 7c dc 84", "sub", "sp, sp, #0x30"),
        (0x00410c48, "d0 75 dc 80", "mov", "r4, r0"),
        (0x00410c4c, "b4 88 dc 8c", "mov", "r5, r1"),
        (0x00410c50, "ff c0 de 88", "ldr", "r1, [pc, #0x40]"),
        (0x00410c54, "b3 ba de 84", "mov", "r0, r4"),
        (0x00410c58, "36 39 73 e9", "bl", "0x00413e60"),
        (0x00410c5c, "82 3f 73 ed", "mov", "r2, r5"),
        (0x00410c60, "ce 45 73 f1", "bl", "<EXTERNAL>::socket"),
        (0x00410c64, "65 84 75 f1", "cmp", "r0, #0x0"),
        (0x00410c68, "19 7e 75 ed", "blt", "0x00410c8c"),
        (0x00410c6c, "04 12 6e 7d", "mov", "r0, r6"),
        (0x00410c70, "b8 0b 6e 79", "bl", "<EXTERNAL>::bind"),
        (0x00410c74, "6c 05 6e 75", "mov", "r2, r4"),
        (0x00410c78, "e7 56 70 81", "bl", "<EXTERNAL>::listen"),
        (0x00410c7c, "9b 50 70 7d", "cmp", "r0, #0x0"),
        (0x00410c80, "72 b8 68 f1", "blt", "0x00410ca4"),
        (0x00410c84, "26 b2 68 ed", "mov", "r0, r5"),
        (0x00410c88, "0a c5 68 f9", "bl", "0x00415a20"),
        (0x00410c8c, "21 cd 6b 79", "mov", "r2, r6"),
        (0x00410c90, "6d d3 6b 7d", "bl", "<EXTERNAL>::accept"),
        (0x00410c94, "60 28 64 e5", "cmp", "r0, #0x0"),
        (0x00410c98, "ac 2e 64 e9", "blt", "0x00410cbc"),
        (0x00410c9c, "f8 34 64 ed", "mov", "r0, r4"),
        (0x00410ca0, "43 6d 66 e9", "bl", "0x00411f04"),
        (0x00410ca4, "8f 73 66 ed", "mov", "r0, #0x0"),
        (0x00410ca8, "1e da 86 f9", "add", "sp, sp, #0x30"),
        (0x00410cac, "6a e0 86 fd", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("handle_client", &[
        (0x00411f04, "c5 ff f6 c8", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00411f08, "79 f9 f6 c4", "sub", "sp, sp, #0x20"),
        (0x00411f0c, "5d 0c f7 d0", "mov", "r4, r0"),
        (0x00411f10, "e2 ba f4 c4", "mov", "r5, r1"),
        (0x00411f14, "96 b4 f4 c0", "ldr", "r1, [pc, #0x40]"),
        (0x00411f18, "88 f9 b7 38", "mov", "r0, r4"),
        (0x00411f1c, "d4 ff b7 3c", "bl", "<EXTERNAL>::memset"),
        (0x00411f20, "f0 ec b7 30", "ldr", "r1, [pc, #0x48]"),
        (0x00411f24, "6b 3e ba 3c", "mov", "r2, r5"),
        (0x00411f28, "b7 44 ba 40", "bl", "0x00412a30"),
        (0x00411f2c, "1a 53 bd c4", "cmp", "r0, #0x0"),
        (0x00411f30, "ce 4c bd c0", "blt", "0x00411f54"),
        (0x00411f34, "82 46 bd bc", "mov", "r0, r6"),
        (0x00411f38, "e9 6b bf ac", "bl", "0x00413900"),
        (0x00411f3c, "35 72 bf b0", "mov", "r2, r4"),
        (0x00411f40, "cc b0 c1 b0", "bl", "0x00414400"),
        (0x00411f44, "80 aa c1 ac", "cmp", "r0, #0x0"),
        (0x00411f48, "64 bd c1 b8", "blt", "0x00411f6c"),
        (0x00411f4c, "2f bf c4 34", "mov", "r0, r5"),
        (0x00411f50, "e3 b8 c4 30", "bl", "0x00413480"),
        (0x00411f54, "5e 0a c7 3c", "mov", "r2, r6"),
        (0x00411f58, "aa 10 c7 40", "bl", "<EXTERNAL>::close"),
        (0x00411f5c, "c6 fd c6 34", "cmp", "r0, #0x0"),
        (0x00411f60, "8d 55 c9 44", "blt", "0x00411f84"),
        (0x00411f64, "41 4f c9 40", "mov", "r0, #0x0"),
        (0x00411f68, "a0 58 a4 28", "add", "sp, sp, #0x20"),
        (0x00411f6c, "ec 5e a4 2c", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("parse_request", &[
        (0x00412a30, "a7 96 0e c4", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00412a34, "5b 90 0e c0", "sub", "sp, sp, #0x30"),
        (0x00412a38, "0f 8a 0e bc", "mov", "r4, r0"),
        (0x00412a3c, "78 4b 0c bc", "mov", "r5, r1"),
        (0x00412a40, "c4 51 0c c0", "ldr", "r1, [pc, #0x40]"),
        (0x00412a44, "95 06 0a b8", "mov", "r0, r4"),
        (0x00412a48, "49 00 0a b4", "bl", "0x00412e10"),
        (0x00412a4c, "2d 13 0a c0", "ldr", "r1, [pc, #0x48]"),
        (0x00412a50, "32 f8 06 34", "mov", "r2, r5"),
        (0x00412a54, "e6 f1 06 30", "bl", "<EXTERNAL>::sscanf"),
        (0x00412a58, "7b 3e f1 3b", "cmp", "r0, #0x0"),
        (0x00412a5c, "c7 44 f1 3f", "blt", "0x00412a80"),
        (0x00412a60, "e3 31 f1 33", "ldr", "r1, [pc, #0x50]"),
        (0x00412a64, "cc 29 ee b3", "mov", "r0, r6"),
        (0x00412a68, "80 23 ee af", "bl", "0x00413480"),
        (0x00412a6c, "b8 44 30 cc", "ldr", "r1, [pc, #0x58]"),
        (0x00412a70, "04 4b 30 d0", "mov", "r2, r4"),
        (0x00412a74, "20 38 30 c4", "bl", "0x00413120"),
        (0x00412a78, "9b 89 32 d0", "cmp", "r0, #0x0"),
        (0x00412a7c, "e7 8f 32 d4", "blt", "0x00412aa0"),
        (0x00412a80, "ca d4 34 d8", "mov", "r0, r5"),
        (0x00412a84, "7e ce 34 d4", "bl", "<EXTERNAL>::strncasecmp"),
        (0x00412a88, "32 c8 34 d0", "mov", "r2, r6"),
        (0x00412a8c, "19 b7 37 40", "bl", "<EXTERNAL>::atoi"),
        (0x00412a90, "65 bd 37 44", "cmp", "r0, #0x0"),
        (0x00412a94, "fc fb 39 44", "blt", "0x00412ab8"),
        (0x00412a98, "b0 f5 39 40", "mov", "r0, r4"),
        (0x00412a9c, "94 08 3a 4c", "bl", "<EXTERNAL>::strncpy"),
        (0x00412aa0, "df 40 3c 48", "mov", "r2, r5"),
        (0x00412aa4, "93 3a 3c 44", "bl", "<EXTERNAL>::recv"),
        (0x00412aa8, "8e 55 3f d0", "cmp", "r0, #0x0"),
        (0x00412aac, "da 5b 3f d4", "blt", "0x00412ad0"),
        (0x00412ab0, "f6 48 3f c8", "mov", "r0, #0x0"),
        (0x00412ab4, "bd a0 41 d8", "add", "sp, sp, #0x30"),
        (0x00412ab8, "71 9a 41 d4", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("read_line", &[
        (0x00412e10, "55 8b fe 66", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00412e14, "09 85 fe 62", "sub", "sp, sp, #0x20"),
        (0x00412e18, "50 2f ed 5a", "mov", "r4, r0"),
        (0x00412e1c, "9c 35 ed 5e", "mov", "r5, r1"),
        (0x00412e20, "e8 3b ed 62", "mov", "r0, r4"),
        (0x00412e24, "33 74 ef 5e", "bl", "<EXTERNAL>::recv"),
        (0x00412e28, "7f 7a ef 62", "mov", "r0, #0x0"),
        (0x00412e2c, "62 bf f1 66", "add", "sp, sp, #0x20"),
        (0x00412e30, "16 b9 f1 62", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("url_decode", &[
        (0x00413120, "c4 04 99 3e", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00413124, "95 b9 96 36", "sub", "sp, sp, #0x20"),
        (0x00413128, "49 b3 96 32", "mov", "r4, r0"),
        (0x0041312c, "2d c6 96 3e", "mov", "r5, r1"),
        (0x00413130, "32 ab 93 b2", "ldr", "r1, [pc, #0x40]"),
        (0x00413134, "e6 a4 93 ae", "mov", "r0, r4"),
        (0x00413138, "03 60 91 aa", "bl", "<EXTERNAL>::isxdigit"),
        (0x0041313c, "4f 66 91 ae", "mov", "r2, r5"),
        (0x00413140, "9b 6c 91 b2", "bl", "<EXTERNAL>::sscanf"),
        (0x00413144, "34 47 8f c2", "cmp", "r0, #0x0"),
        (0x00413148, "e8 40 8f be", "blt", "0x0041316c"),
        (0x0041314c, "d1 38 8c 3e", "mov", "r0, #0x0"),
        (0x00413150, "1d 3f 8c 42", "add", "sp, sp, #0x20"),
        (0x00413154, "39 2c 8c 36", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("send_response", &[
        (0x00413480, "d9 41 2f 97", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00413484, "25 48 2f 9b", "sub", "sp, sp, #0x20"),
        (0x00413488, "71 4e 2f 9f", "mov", "r4, r0"),
        (0x0041348c, "f6 fc 2c 93", "mov", "r5, r1"),
        (0x00413490, "42 03 2d 97", "ldr", "r1, [pc, #0x40]"),
        (0x00413494, "96 7c ab 28", "mov", "r0, r4"),
        (0x00413498, "e2 82 ab 2c", "bl", "<EXTERNAL>::snprintf"),
        (0x0041349c, "2e 89 ab 30", "ldr", "r1, [pc, #0x48]"),
        (0x004134a0, "c5 c7 ad 30", "mov", "r2, r5"),
        (0x004134a4, "79 c1 ad 2c", "bl", "<EXTERNAL>::strlen"),
        (0x004134a8, "64 55 a6 bc", "cmp", "r0, #0x0"),
        (0x004134ac, "18 4f a6 b8", "blt", "0x004134d0"),
        (0x004134b0, "cc 48 a6 b4", "ldr", "r1, [pc, #0x50]"),
        (0x004134b4, "c7 63 a9 40", "mov", "r0, r6"),
        (0x004134b8, "7b 5d a9 3c", "bl", "<EXTERNAL>::send"),
        (0x004134bc, "52 c5 a1 b0", "mov", "r0, #0x0"),
        (0x004134c0, "06 bf a1 ac", "add", "sp, sp, #0x20"),
        (0x004134c4, "ea d1 a1 b8", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("check_session", &[
        (0x00413900, "aa c0 90 a7", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00413904, "5e ba 90 a3", "sub", "sp, sp, #0x10"),
        (0x00413908, "7b 75 8e 9f", "mov", "r4, r0"),
        (0x0041390c, "c7 7b 8e a3", "mov", "r5, r1"),
        (0x00413910, "e3 68 8e 97", "mov", "r0, r4"),
        (0x00413914, "cc 60 8b 17", "bl", "0x00413b10"),
        (0x00413918, "80 5a 8b 13", "mov", "r2, r5"),
        (0x0041391c, "e9 1b 89 13", "bl", "<EXTERNAL>::strcmp"),
        (0x00413920, "35 22 89 17", "cmp", "r0, #0x0"),
        (0x00413924, "81 28 89 1b", "blt", "0x00413948"),
        (0x00413928, "06 d7 86 0f", "mov", "r0, #0x0"),
        (0x0041392c, "52 dd 86 13", "add", "sp, sp, #0x10"),
        (0x00413930, "a7 cd ab 27", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("FUN_00413b10", &[
        (0x00413b10, "3a 3a fc ce", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00413b14, "ee 33 fc ca", "sub", "sp, sp, #0x20"),
        (0x00413b18, "a2 2d fc c6", "mov", "r4, r0"),
        (0x00413b1c, "89 1c ff 36", "mov", "r5, r1"),
        (0x00413b20, "d5 22 ff 3a", "ldr", "r1, [pc, #0x40]"),
        (0x00413b24, "64 29 ed ca", "mov", "r0, r4"),
        (0x00413b28, "18 23 ed c6", "bl", "<EXTERNAL>::strstr"),
        (0x00413b2c, "cc 1c ed c2", "mov", "r2, r5"),
        (0x00413b30, "c7 37 f0 4e", "bl", "<EXTERNAL>::strchr"),
        (0x00413b34, "7b 31 f0 4a", "cmp", "r0, #0x0"),
        (0x00413b38, "9e 73 66 7e", "blt", "0x00413b5c"),
        (0x00413b3c, "ea 79 66 82", "mov", "r0, #0x0"),
        (0x00413b40, "06 67 66 76", "add", "sp, sp, #0x20"),
        (0x00413b44, "cd be 68 86", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("load_config", &[
        (0x00413e60, "ec 54 52 35", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00413e64, "67 a6 54 41", "sub", "sp, sp, #0x20"),
        (0x00413e68, "1b a0 54 3d", "mov", "r4, r0"),
        (0x00413e6c, "b6 88 57 a9", "mov", "r5, r1"),
        (0x00413e70, "02 8f 57 ad", "ldr", "r1, [pc, #0x40]"),
        (0x00413e74, "4e 95 57 b1", "mov", "r0, r4"),
        (0x00413e78, "e5 d3 59 b1", "bl", "<EXTERNAL>::fopen"),
        (0x00413e7c, "99 cd 59 ad", "ldr", "r1, [pc, #0x48]"),
        (0x00413e80, "e0 77 48 a5", "mov", "r2, r5"),
        (0x00413e84, "2c 7e 48 a9", "bl", "<EXTERNAL>::fread"),
        (0x00413e88, "78 84 48 ad", "cmp", "r0, #0x0"),
        (0x00413e8c, "c3 bc 4a a9", "blt", "0x00413eb0"),
        (0x00413e90, "0f c3 4a ad", "mov", "r0, r6"),
        (0x00413e94, "f2 07 4d b1", "bl", "<EXTERNAL>::fclose"),
        (0x00413e98, "a6 01 4d ad", "mov", "r2, r4"),
        (0x00413e9c, "8a 14 4d b9", "bl", "<EXTERNAL>::json_parse"),
        (0x00413ea0, "a1 1c 50 39", "cmp", "r0, #0x0"),
        (0x00413ea4, "ed 22 50 3d", "blt", "0x00413ec8"),
        (0x00413ea8, "0c d0 65 2d", "mov", "r0, r5"),
        (0x00413eac, "c0 c9 65 29", "bl", "<EXTERNAL>::json_get_int"),
        (0x00413eb0, "a4 dc 65 35", "mov", "r0, #0x0"),
        (0x00413eb4, "6f de 68 b1", "add", "sp, sp, #0x20"),
        (0x00413eb8, "23 d8 68 ad", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("serve_file", &[
        (0x00414400, "1e cb 8c 6b", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00414404, "b5 09 8f 6b", "sub", "sp, sp, #0x20"),
        (0x00414408, "69 03 8f 67", "mov", "r4, r0"),
        (0x0041440c, "54 97 87 f7", "mov", "r5, r1"),
        (0x00414410, "08 91 87 f3", "ldr", "r1, [pc, #0x40]"),
        (0x00414414, "bc 8a 87 ef", "mov", "r0, r4"),
        (0x00414418, "37 dc 89 fb", "bl", "<EXTERNAL>::snprintf"),
        (0x0041441c, "eb d5 89 f7", "ldr", "r1, [pc, #0x48]"),
        (0x00414420, "2a a8 96 fb", "mov", "r2, r5"),
        (0x00414424, "de a1 96 f7", "bl", "0x004149a0"),
        (0x00414428, "92 9b 96 f3", "cmp", "r0, #0x0"),
        (0x0041442c, "f9 c0 98 e3", "blt", "0x00414450"),
        (0x00414430, "45 c7 98 e7", "ldr", "r1, [pc, #0x50]"),
        (0x00414434, "6d 6d 1a 6a", "mov", "r0, r6"),
        (0x00414438, "21 67 1a 66", "bl", "<EXTERNAL>::open"),
        (0x0041443c, "d5 60 1a 62", "ldr", "r1, [pc, #0x58]"),
        (0x00414440, "0a 5f 17 e6", "mov", "r2, r4"),
        (0x00414444, "be 58 17 e2", "bl", "0x00413480"),
        (0x00414448, "1f cb 1e 56", "cmp", "r0, #0x0"),
        (0x0041444c, "d3 c4 1e 52", "blt", "0x00414470"),
        (0x00414450, "b7 d7 1e 5e", "mov", "r0, r5"),
        (0x00414454, "f0 7f 1c 4e", "bl", "<EXTERNAL>::sendfile"),
        (0x00414458, "3c 86 1c 52", "mov", "r2, r6"),
        (0x0041445c, "c9 83 10 d2", "bl", "<EXTERNAL>::close"),
        (0x00414460, "15 8a 10 d6", "cmp", "r0, #0x0"),
        (0x00414464, "61 90 10 da", "blt", "0x00414488"),
        (0x00414468, "66 75 0d 4e", "mov", "r0, #0x0"),
        (0x0041446c, "b2 7b 0d 52", "add", "sp, sp, #0x20"),
        (0x00414470, "db 13 15 de", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("mime_type", &[
        (0x004149a0, "a3 19 e4 8e", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x004149a4, "8c 11 e1 0e", "sub", "sp, sp, #0x20"),
        (0x004149a8, "40 0b e1 0a", "mov", "r4, r0"),
        (0x004149ac, "78 2c 23 27", "mov", "r5, r1"),
        (0x004149b0, "c4 32 23 2b", "ldr", "r1, [pc, #0x40]"),
        (0x004149b4, "e0 1f 23 1f", "mov", "r0, r4"),
        (0x004149b8, "5b 71 25 2b", "bl", "<EXTERNAL>::strrchr"),
        (0x004149bc, "a7 77 25 2f", "ldr", "r1, [pc, #0x48]"),
        (0x004149c0, "8a bc 27 33", "mov", "r2, r5"),
        (0x004149c4, "3e b6 27 2f", "bl", "<EXTERNAL>::strcmp"),
        (0x004149c8, "f2 af 27 2b", "cmp", "r0, #0x0"),
        (0x004149cc, "d9 9e 2a 9b", "blt", "0x004149f0"),
        (0x004149d0, "25 a5 2a 9f", "ldr", "r1, [pc, #0x80]"),
        (0x004149d4, "bc e3 2c 9f", "ldr", "r1, [pc, #0x80]"),
        (0x004149d8, "70 dd 2c 9b", "ldr", "r1, [pc, #0x80]"),
        (0x004149dc, "54 f0 2c a7", "ldr", "r1, [pc, #0x80]"),
        (0x004149e0, "9f 28 2f a3", "ldr", "r1, [pc, #0x80]"),
        (0x004149e4, "53 22 2f 9f", "mov", "r0, #0x0"),
        (0x004149e8, "4e 3d 32 2b", "add", "sp, sp, #0x20"),
        (0x004149ec, "9a 43 32 2f", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("FUN_00414c88", &[
        (0x00414c88, "1a 99 6d ca", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00414c8c, "ce 92 6d c6", "sub", "sp, sp, #0x20"),
        (0x00414c90, "6b 84 6a 42", "mov", "r4, r0"),
        (0x00414c94, "b7 8a 6a 46", "mov", "r5, r1"),
        (0x00414c98, "d3 77 6a 3a", "mov", "r0, #0x0"),
        (0x00414c9c, "3c 39 68 3a", "add", "sp, sp, #0x20"),
        (0x00414ca0, "f0 32 68 36", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
    ("FUN_00415a20", &[
        (0x00415a20, "50 6f 02 90", "push", "{r4, r5, r6, r7, r8, lr}"),
        (0x00415a24, "9c 75 02 94", "sub", "sp, sp, #0x10"),
        (0x00415a28, "4d 93 ff 2b", "mov", "r4, r0"),
        (0x00415a2c, "01 8d ff 27", "mov", "r5, r1"),
        (0x00415a30, "b5 86 ff 23", "ldr", "r1, [pc, #0x40]"),
        (0x00415a34, "6a 4e fd 27", "mov", "r0, r4"),
        (0x00415a38, "1e 48 fd 23", "bl", "<EXTERNAL>::open"),
        (0x00415a3c, "3b 03 fb 1f", "ldr", "r1, [pc, #0x48]"),
        (0x00415a40, "87 09 fb 23", "mov", "r2, r5"),
        (0x00415a44, "a3 f6 fa 17", "bl", "<EXTERNAL>::dprintf"),
        (0x00415a48, "8c ee f7 97", "cmp", "r0, #0x0"),
        (0x00415a4c, "40 e8 f7 93", "blt", "0x00415a70"),
        (0x00415a50, "a9 a9 f5 93", "mov", "r0, r6"),
        (0x00415a54, "f5 af f5 97", "bl", "<EXTERNAL>::getpid"),
        (0x00415a58, "41 b6 f5 9b", "mov", "r2, r4"),
        (0x00415a5c, "c6 64 f3 8f", "bl", "<EXTERNAL>::close"),
        (0x00415a60, "12 6b f3 93", "cmp", "r0, #0x0"),
        (0x00415a64, "67 5b 18 a8", "blt", "0x00415a88"),
        (0x00415a68, "1b 55 18 a4", "mov", "r0, #0x0"),
        (0x00415a6c, "cf 4e 18 a0", "add", "sp, sp, #0x10"),
        (0x00415a70, "38 10 16 a0", "pop", "{r4, r5, r6, r7, r8, pc}"),
    ]),
];

// The Electron build's decompiled functions: name, plate comment, C.
pub const DECOMPILED: &[(&str, Option<&str>, &str)] = &[
    (
        "main",
        None,
        r#"int main(int argc, char **argv)
{
  int server;
  int client;
  struct sockaddr_in addr;
  socklen_t addr_len;

  load_config("/etc/config.json");
  server = socket(AF_INET, SOCK_STREAM, 0);
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(g_config.port);
  bind(server, (struct sockaddr *)&addr, sizeof(addr));
  listen(server, 16);
  FUN_00415a20();
  while (true) {
    addr_len = sizeof(addr);
    client = accept(server, (struct sockaddr *)&addr, &addr_len);
    if (-1 < client) {
      handle_client(client);
    }
  }
}"#,
    ),
    (
        "handle_client",
        Some("One connection at a time, served inline."),
        r#"void handle_client(int fd)
{
  conn_t conn;
  char buf[1024];

  memset(&conn, 0, sizeof(conn));
  conn.fd = fd;
  if (parse_request(&conn, buf, sizeof(buf)) < 0) {
    close(fd);
    return;
  }
  if (strncmp(conn.path, "/api/", 5) == 0 && !check_session(&conn)) {
    send_response(&conn, 401, "Unauthorized");
  }
  else {
    serve_file(&conn, conn.path);
  }
  close(fd);
}"#,
    ),
    (
        "parse_request",
        Some("Request line, then headers until the blank line."),
        r#"int parse_request(conn_t *conn, char *buf, int len)
{
  char *line;
  int content_length;

  line = read_line(conn, buf, len);
  if (line == NULL) {
    return -1;
  }
  if (sscanf(line, "%7s %255s HTTP/1.%*d", conn->method, conn->path) != 2) {
    send_response(conn, 400, "Bad Request");
    return -1;
  }
  url_decode(conn->path);

  content_length = 0;
  while ((line = read_line(conn, buf, len)) != NULL && *line != '\0') {
    if (strncasecmp(line, "Content-Length:", 15) == 0) {
      content_length = atoi(line + 15);
    }
    else if (strncasecmp(line, "Cookie:", 7) == 0) {
      strncpy(conn->cookie, line + 7, 127);
    }
  }
  if (0 < content_length) {
    conn->body_len = recv(conn->fd, conn->body, content_length, 0);
  }
  return 0;
}"#,
    ),
    (
        "read_line",
        None,
        r#"char * read_line(conn_t *conn, char *buf, int len)
{
  int i;
  char c;

  for (i = 0; i < len + -1; i = i + 1) {
    if (recv(conn->fd, &c, 1, 0) != 1) {
      return NULL;
    }
    if (c == '\n') break;
    if (c != '\r') {
      buf[i] = c;
    }
  }
  buf[i] = '\0';
  return buf;
}"#,
    ),
    (
        "url_decode",
        None,
        r#"void url_decode(char *s)
{
  char *out;
  uint value;

  out = s;
  while (*s != '\0') {
    if (*s == '%' && isxdigit(s[1]) != 0 && isxdigit(s[2]) != 0) {
      sscanf(s + 1, "%2x", &value);
      *out = (char)value;
      s = s + 3;
    }
    else {
      *out = *s == '+' ? ' ' : *s;
      s = s + 1;
    }
    out = out + 1;
  }
  *out = '\0';
}"#,
    ),
    (
        "send_response",
        None,
        r#"int send_response(conn_t *conn, int status, char *body)
{
  char header[256];
  int n;

  n = snprintf(header, sizeof(header),
               "HTTP/1.1 %d %s\r\nContent-Length: %d\r\nConnection: close\r\n\r\n",
               status, status == 200 ? "OK" : "Error", strlen(body));
  send(conn->fd, header, n, 0);
  return send(conn->fd, body, strlen(body), 0);
}"#,
    ),
    (
        "check_session",
        None,
        r#"bool check_session(conn_t *conn)
{
  char *token;

  token = FUN_00413b10(conn->cookie);
  if (token == NULL) {
    return false;
  }
  return strcmp(token, g_session_token) == 0;
}"#,
    ),
    (
        "FUN_00413b10",
        None,
        r#"char * FUN_00413b10(char *param_1)
{
  char *pcVar1;
  char *pcVar2;

  pcVar1 = strstr(param_1, "session=");
  if (pcVar1 == NULL) {
    return NULL;
  }
  pcVar1 = pcVar1 + 8;
  pcVar2 = strchr(pcVar1, ';');
  if (pcVar2 != NULL) {
    *pcVar2 = '\0';
  }
  return pcVar1;
}"#,
    ),
    (
        "load_config",
        None,
        r#"int load_config(char *path)
{
  FILE *fp;
  char text[2048];
  json_t *root;

  fp = fopen(path, "r");
  if (fp == NULL) {
    return -1;
  }
  text[fread(text, 1, sizeof(text) - 1, fp)] = '\0';
  fclose(fp);
  root = json_parse(text);
  g_config.port = json_get_int(root, "httpd.port", 80);
  return 0;
}"#,
    ),
    (
        "serve_file",
        None,
        r#"int serve_file(conn_t *conn, char *path)
{
  char full[512];
  int fd;

  snprintf(full, sizeof(full), "/www%s", strcmp(path, "/") == 0 ? "/index.html" : path);
  fd = open(full, O_RDONLY);
  if (fd < 0) {
    return send_response(conn, 404, "Not Found");
  }
  conn->content_type = mime_type(full);
  sendfile(conn->fd, fd, NULL, 0x10000);
  close(fd);
  return 0;
}"#,
    ),
    (
        "mime_type",
        None,
        r#"char * mime_type(char *path)
{
  char *ext;

  ext = strrchr(path, '.');
  if (ext == NULL) {
    return "application/octet-stream";
  }
  if (strcmp(ext, ".html") == 0) {
    return "text/html";
  }
  if (strcmp(ext, ".js") == 0) {
    return "text/javascript";
  }
  if (strcmp(ext, ".css") == 0) {
    return "text/css";
  }
  return "application/octet-stream";
}"#,
    ),
    (
        "FUN_00414c88",
        None,
        r#"uint FUN_00414c88(uint param_1, byte *param_2, int param_3)
{
  int iVar1;

  while (param_3 != 0) {
    param_1 = param_1 ^ *param_2;
    for (iVar1 = 0; iVar1 < 8; iVar1 = iVar1 + 1) {
      param_1 = param_1 >> 1 ^ -(param_1 & 1) & 0xedb88320;
    }
    param_2 = param_2 + 1;
    param_3 = param_3 + -1;
  }
  return ~param_1;
}"#,
    ),
    (
        "FUN_00415a20",
        None,
        r#"void FUN_00415a20(void)
{
  int fd;

  fd = open("/var/run/httpd.pid", O_WRONLY | O_CREAT | O_TRUNC, 0644);
  if (-1 < fd) {
    dprintf(fd, "%d\n", getpid());
    close(fd);
  }
}"#,
    ),
];
