//! Generates, into `OUT_DIR`:
//!
//! 1. the buffa models of `proto/demo/v1/svc.proto` (with views), into `models/_gen/`;
//! 2. the trestle REST client (reqwest transport, error module) into `client/`, and the models
//!    `mod.rs` (with the `json_int64` helpers and the PyO3 wrappers) next to the buffa files;
//! 3. the trestle model rewrites over the buffa files: snake_case JSON names, int64 as numbers.
//!
//! `src/lib.rs` mounts both trees. The Python bindings are built by the `py` crate.

include!("config.rs");

use olai_codegen::{generate_code, parse_file_descriptor_set};
use protobuf::Message;
use protobuf::descriptor::FileDescriptorSet;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptors = compile_descriptors(&out);
    let config = demo_config(&out);
    let gen_dir = config.output.models_subdir_path().unwrap();

    // 1. buffa models. buffa-build writes one file per proto file plus `<package>.mod.rs`; the
    // buf plugin's `file_per_package` writes `<package>.rs`, which the models `mod.rs` includes.
    buffa_build::Config::new()
        .files(&[fixture_dir().join("proto/demo/v1/svc.proto")])
        .includes(&[fixture_dir().join("proto")])
        .generate_json(true)
        .generate_views(true)
        .out_dir(&gen_dir)
        .include_file("_include.rs")
        .compile()
        .unwrap();
    std::fs::write(
        gen_dir.join("demo.v1.rs"),
        "include!(\"demo.v1.mod.rs\");\n",
    )
    .unwrap();

    // 2. trestle client + models `mod.rs`.
    let fds = FileDescriptorSet::parse_from_bytes(&std::fs::read(&descriptors).unwrap()).unwrap();
    let metadata = parse_file_descriptor_set(&fds).unwrap();
    generate_code(&metadata, &config).unwrap();

    // 3. The model rewrites.
    demo_rewrites(&config, &metadata).rewrite_dir(&gen_dir).unwrap();

    // `#[path]` needs a literal path, so mount the generated `mod.rs` through a small file.
    std::fs::write(
        out.join("models_mod.rs"),
        format!(
            "#[path = {:?}]\npub mod _gen;\n",
            gen_dir.join("mod.rs").display().to_string()
        ),
    )
    .unwrap();
}
