//! Generates the `lumia.v1` types from the schemas in `proto/` at the
//! repository root. `protox` compiles the schemas in pure Rust, so no
//! `protoc` is needed on the machine.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_root = manifest.join("../../../../proto").canonicalize()?;
    let files = [
        "lumia/v1/common.proto",
        "lumia/v1/identity.proto",
        "lumia/v1/chat.proto",
        "lumia/v1/events.proto",
        "lumia/v1/commands.proto",
        "lumia/v1/gateway.proto",
        "lumia/v1/api.proto",
        "lumia/v1/e2ee.proto",
    ];

    for file in &files {
        println!("cargo:rerun-if-changed={}", proto_root.join(file).display());
    }

    let descriptors = protox::compile(files, [&proto_root])?;
    prost_build::Config::new().compile_fds(descriptors)?;
    Ok(())
}
