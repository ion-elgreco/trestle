//! Generates the Python bindings (`python/`) and the Python package files (`pkg/demo/`) into
//! `OUT_DIR`, from the same config as the client crate.

include!("../config.rs");

use olai_codegen::{generate_code, parse_file_descriptor_set};
use protobuf::Message;
use protobuf::descriptor::FileDescriptorSet;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptors = compile_descriptors(&out);
    let config = demo_config(&out);
    let fds = FileDescriptorSet::parse_from_bytes(&std::fs::read(&descriptors).unwrap()).unwrap();
    let metadata = parse_file_descriptor_set(&fds).unwrap();
    generate_code(&metadata, &config).unwrap();
    std::fs::write(out.join("pkg/demo/__init__.py"), "").unwrap();

    // `#[path]` needs a literal path, so mount the generated `mod.rs` through a small file.
    std::fs::write(
        out.join("python_mod.rs"),
        format!(
            "#[path = {:?}]\npub mod codegen;\n",
            out.join("python/mod.rs").display().to_string()
        ),
    )
    .unwrap();
}
