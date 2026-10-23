// Shared by `build.rs` and `py/build.rs` (each `include!`s it): the generation config of the
// fixture, with every client and Python option this fixture tests turned on.

use std::path::{Path, PathBuf};
use std::process::Command;

use olai_codegen::postprocess::{JsonFieldNames, ModelRewrites};
use olai_codegen::{
    BindingsConfig, CodeGenConfig, CodeGenOutput, ErrorModuleConfig, PyConstructors,
    PyErrorModuleConfig, PyPackageConfig, PythonOptions, REQWEST_TRANSPORT_TYPE_PATH, RestClientOptions, Runtime,
};

/// The fixture directory (the one with `proto/`).
fn fixture_dir() -> PathBuf {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    if manifest.join("proto").exists() {
        manifest
    } else {
        manifest.parent().unwrap().to_path_buf()
    }
}

/// Compile the proto to a descriptor set (with source info for the doc comments).
fn compile_descriptors(out: &Path) -> PathBuf {
    let fixture = fixture_dir();
    let descriptors = out.join("svc.bin");
    let status = Command::new("protoc")
        .args(["--include_imports", "--include_source_info", "-I"])
        .arg(fixture.join("proto"))
        .arg(format!("--descriptor_set_out={}", descriptors.display()))
        .arg(fixture.join("proto/demo/v1/svc.proto"))
        .status()
        .expect("protoc on PATH");
    assert!(status.success(), "protoc failed");
    println!("cargo:rerun-if-changed={}", fixture.join("proto").display());
    println!("cargo:rerun-if-changed={}", fixture.join("../../src").display());
    descriptors
}

/// The generation config. Everything goes under `out`; the Python package files under
/// `out/pkg/demo`.
fn demo_config(out: &Path) -> CodeGenConfig {
    let models_dir = out.join("models");
    let gen_dir = models_dir.join("_gen");
    let package_dir = out.join("pkg").join("demo");
    for dir in [&gen_dir, &out.join("client"), &out.join("python"), &package_dir] {
        std::fs::create_dir_all(dir).unwrap();
    }
    CodeGenConfig {
        context_type_path: "crate::Context".into(),
        // The clients use the `Result` of the generated error module, not this one.
        result_type_path: "crate::Result".into(),
        models_path_template: "rest_client_e2e::models::{service}::{version}".into(),
        models_path_crate_template: "crate::models::{service}::{version}".into(),
        resource_store_crate_name: "olai_store".into(),
        runtime: Runtime::Buffa,
        transport_type_path: REQWEST_TRANSPORT_TYPE_PATH.into(),
        dual_transport: false,
        client_protocols: olai_codegen::ClientProtocols::default(),
        connect_client_path: None,
        rest_client: RestClientOptions {
            json_int64_as_number: true,
            ignore_unknown_enum_values: true,
            error_module: Some(ErrorModuleConfig {
                code_enum: Some(".demo.v1.ErrorCode".into()),
                ..Default::default()
            }),
            form_encoded_methods: vec!["demo.v1.TokenService.ExchangeToken".into()],
            json_query_packages: vec![],
        },
        output: CodeGenOutput {
            common: gen_dir.clone(),
            models: Some(models_dir),
            models_subdir: "_gen".into(),
            server: None,
            client: Some(out.join("client")),
            generate_resource_clients: true,
            python: Some(out.join("python")),
            node: None,
            node_ts: None,
            wasm: None,
            python_typings_filename: "client.pyi".into(),
        },
        generate_resource_enum: false,
        generate_store_integration: false,
        error_type_path: None,
        generate_object_conversions: false,
        bindings: Some(BindingsConfig {
            aggregate_client_name: "DemoClient".into(),
            client_crate_name: "rest_client_e2e".into(),
            py_error_type: "PyDemoError".into(),
            py_result_type: "PyDemoResult".into(),
            napi_error_ext_trait: "NapiErrorExt".into(),
            typings_package_filter: None,
            ts_error_base_class: "DemoError".into(),
            ts_error_code_prefix: "DEMO".into(),
            python: PythonOptions {
                frozen_models: true,
                constructors: PyConstructors::Inputs,
                python_repr: true,
                module: Some("demo._core".into()),
                async_methods: true,
                runtime: true,
                keyword_only: true,
                str_enums: true,
                hide_unspecified: true,
                stub_gen: true,
                error_module: Some(PyErrorModuleConfig {
                    base_class: "DemoError".into(),
                }),
                package: Some(PyPackageConfig {
                    dir: package_dir,
                    name: "demo".into(),
                }),
            },
        }),
    }
}

/// The rewrites the `trestle` CLI applies to the buffa output after `buf generate`.
#[allow(dead_code)]
fn demo_rewrites(config: &CodeGenConfig) -> ModelRewrites {
    ModelRewrites {
        field_names: JsonFieldNames::Proto,
        packages: Default::default(),
        int64_module: config.json_int64_module_path(),
    }
}
