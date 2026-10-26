//! Rust types generated from the shared Protocol Buffers schema.
//!
//! These wire types are kept separate from `reve-model`; the adapter will
//! convert between them when it reads Ghidra's exported analysis result.

/// Types generated from `reve.engine.v1` in `reve-engine/proto/engine.proto`.
pub mod proto {
    pub mod reve {
        pub mod engine {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/reve.engine.v1.rs"));
            }
        }
    }
}

/// Configuration for locating and running Ghidra Headless.
pub mod config;

/// Errors returned by the Ghidra adapter.
pub mod error;

/// Read Ghidra's protobuf export and convert it to the shared Rust model.
pub mod export;

/// High-level Ghidra analysis operation.
pub mod orchestrator;

/// Per-analysis temporary project and export files.
pub(crate) mod job;

/// Launch and wait for Ghidra Headless.
pub(crate) mod headless;

pub use config::GhidraConfig;
pub use error::ParserError;
pub use orchestrator::GhidraAdapter;

#[cfg(test)]
mod test_support;
