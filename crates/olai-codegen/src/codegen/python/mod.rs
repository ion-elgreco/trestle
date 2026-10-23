//! Python code generation module
//!
//! Split into two submodules:
//! - `bindings`: PyO3 binding generation (Rust → Python wrapper structs)
//! - `typings`: `.pyi` type stub generation for Python IDE support

mod bindings;
mod errors;
mod future;
mod model_conversions;
mod package;
mod runtime;
mod typings;

pub(crate) use bindings::{generate, main_module};
pub(crate) use model_conversions::generate_pyo3_impls;
pub(crate) use typings::generate_typings;

/// The extra files of the Python bindings, by path relative to the binding output: the error
/// module, the `PyFuture` helper and the runtime, when their options are on.
pub(crate) fn helper_files(config: &crate::codegen::CodeGenConfig) -> Vec<(String, String)> {
    let Some(bindings) = config.bindings.as_ref() else {
        return Vec::new();
    };
    let python = &bindings.python;
    let mut files = Vec::new();
    if python.error_module {
        let package = python
            .package
            .as_ref()
            .expect("validated: the Python `error_module` has a package");
        files.push(("error.rs".to_string(), errors::generate(bindings, package)));
    }
    if python.async_methods {
        files.push((
            "py_future.rs".to_string(),
            future::generate(python.stub_gen, python.runtime),
        ));
    }
    if python.runtime {
        files.push((
            "runtime.rs".to_string(),
            runtime::generate(python.async_methods),
        ));
    }
    files
}

/// The pure-Python files of the package (`_enums.py`, `_errors.py`), by file name, when their
/// options are on.
pub(crate) fn package_files(
    config: &crate::codegen::CodeGenConfig,
    plan: &crate::analysis::GenerationPlan,
    metadata: &crate::parsing::CodeGenMetadata,
) -> Vec<(String, String)> {
    let python = config.python_options();
    let Some(package) = python.package.as_ref() else {
        return Vec::new();
    };
    let mut files = Vec::new();
    if python.str_enums {
        let services: Vec<_> = plan.services.iter().collect();
        let classes = model_conversions::ModelClasses::new(&services, metadata, python);
        files.push((
            "_enums.py".to_string(),
            package::enums_py(metadata, &classes.enums, python),
        ));
    }
    if python.error_module {
        files.push((
            "_errors.py".to_string(),
            package::errors_py(&package.error_base_class),
        ));
    }
    files
}

/// The `pyo3-stub-gen` attribute that goes before a `#[pyclass]`, with
/// [`PythonOptions::stub_gen`].
fn stub_class_attr(options: &PythonOptions) -> Option<TokenStream> {
    options
        .stub_gen
        .then(|| quote! { #[::pyo3_stub_gen::derive::gen_stub_pyclass] })
}

/// The `pyo3-stub-gen` attribute that goes before a `#[pymethods]`, with
/// [`PythonOptions::stub_gen`].
fn stub_methods_attr(options: &PythonOptions) -> Option<TokenStream> {
    options
        .stub_gen
        .then(|| quote! { #[::pyo3_stub_gen::derive::gen_stub_pymethods] })
}

/// The `module = "…",` argument of a `#[pyclass]` with [`PythonOptions::module`]: the module
/// itself, or its submodule of the models segment `segment` of a proto package.
fn module_arg(options: &PythonOptions, segment: Option<&str>) -> Option<TokenStream> {
    let root = options.module.as_deref()?;
    let module = match segment {
        Some(segment) => format!("{root}.{segment}"),
        None => root.to_string(),
    };
    Some(quote! { module = #module, })
}

/// The Python submodule segment of a proto package: its models segment
/// (`a.catalogs.v1` → `catalogs`).
fn package_segment(package: &str) -> &str {
    crate::codegen::package_models_segment(package).unwrap_or(package)
}

use proc_macro2::TokenStream;
use quote::quote;

use crate::analysis::RequestType;
use crate::codegen::{MethodHandler, PythonOptions};
use crate::parsing::types::UnifiedType;
use crate::utils::extract_simple_type_name;

static DOCS_TARGET_WIDTH: usize = 100;

/// Extract parameter names from a `ResourceDescriptor` pattern string.
///
/// For example, `"catalogs/{catalog}/schemas/{schema}"` yields
/// `["catalog_name", "schema_name"]`.
///
/// A single-parameter resource returns `["name"]` for brevity.
pub(super) fn resource_pattern_params(pattern: &str) -> Vec<String> {
    let params: Vec<String> = pattern
        .split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| {
            let inner = &seg[1..seg.len() - 1];
            format!("{inner}_name")
        })
        .collect();

    if params.len() == 1 {
        vec!["name".to_string()]
    } else {
        params
    }
}

fn is_list_method(method: &MethodHandler<'_>) -> bool {
    matches!(method.plan.request_type, RequestType::List)
}

fn python_type_annotation(unified_type: &UnifiedType) -> String {
    crate::parsing::types::unified_to_python_type(unified_type)
}

fn python_type_annotation_from_ident(ident: &syn::Ident) -> String {
    let type_str = ident.to_string();
    match type_str.as_str() {
        "String" => "str".to_string(),
        "i32" | "i64" => "int".to_string(),
        "f32" | "f64" => "float".to_string(),
        "bool" => "bool".to_string(),
        "Vec < u8 >" => "bytes".to_string(),
        "()" => "None".to_string(),
        _ => {
            if let Some(simple_name) = type_str.split("::").last() {
                simple_name.trim().to_string()
            } else {
                type_str
            }
        }
    }
}

fn sanitize_python_field_name(field_name: &str) -> String {
    match field_name {
        "not" | "and" | "or" | "is" | "in" | "def" | "class" | "if" | "else" | "for" | "while"
        | "try" | "except" | "finally" | "with" | "as" | "import" | "from" | "pass" | "break"
        | "continue" | "return" | "yield" | "raise" | "assert" | "del" | "global" | "nonlocal"
        | "lambda" | "None" | "True" | "False" | "async" | "await" => {
            format!("{field_name}_")
        }
        _ => field_name.to_string(),
    }
}

fn clean_and_format_description(text: &str) -> String {
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return String::new();
    }

    // Split on blank lines to preserve paragraph structure
    let paragraphs: Vec<String> = cleaned
        .split("\n\n")
        .map(|para| {
            para.lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|para| !para.is_empty())
        .map(|para| textwrap::fill(&para, DOCS_TARGET_WIDTH))
        .collect();

    paragraphs.join("\n\n")
}
