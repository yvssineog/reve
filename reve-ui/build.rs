use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/reve.ico");
    println!("cargo:rerun-if-changed=assets/reve.rc");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let assets = Path::new("assets");
        embed_resource::compile(
            assets.join("reve.rc"),
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("could not embed the rêve Windows icon");
    }
}
