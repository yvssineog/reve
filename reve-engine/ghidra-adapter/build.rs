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
    println!("cargo:rerun-if-env-changed=PROTOC");
    println!("cargo:rerun-if-env-changed=PATH");

    let mut config = prost_build::Config::new();
    config.protoc_executable(find_protoc()?);
    config.compile_protos(&[proto_file], &[proto_include])?;

    Ok(())
}

/// Prefer an explicitly configured or installed `protoc`; use the vendored
/// binary when the developer does not have one installed.
fn find_protoc() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("PROTOC") {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = find_on_path() {
        return Ok(path);
    }

    Ok(protoc_bin_vendored::protoc_bin_path()?)
}

fn find_on_path() -> Option<PathBuf> {
    let executable = if cfg!(windows) {
        "protoc.exe"
    } else {
        "protoc"
    };

    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(executable))
        .find(|candidate| candidate.is_file())
}
