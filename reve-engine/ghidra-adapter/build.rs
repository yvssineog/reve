use std::env;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let proto_file = manifest_dir.join("../proto/engine.proto");
    let proto_include = proto_file
        .parent()
        .expect("the protobuf schema path should have a parent directory")
        .to_path_buf();

    println!("cargo:rerun-if-changed={}", proto_file.display());

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.compile_protos(&[proto_file], &[proto_include])?;

    Ok(())
}
