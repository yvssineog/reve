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
