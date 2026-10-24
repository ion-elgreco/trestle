//! PyO3 binding generation for protobuf-defined services.
//!
//! The dispatch and "which methods land where" logic is shared with the Node emitter via
//! [`crate::codegen::bindings`]; this module supplies the PyO3-specific scaffold, marshaling, and
//! method bodies through [`PyBackend`]'s [`BindingBackend`] impl.

use std::collections::BTreeMap;

use itertools::Itertools;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

use super::super::format_tokens;
use crate::codegen::bindings::builder::{OptionalSetter, SetterRender, generate_builder_pattern};
use crate::codegen::bindings::{BindingBackend, ShapeParts, driver};
use crate::codegen::{BindingMode, MethodHandler, PythonOptions, ServiceHandler};
use crate::parsing::types::{BaseType, JsonType, RenderContext, UnifiedType};

/// The `Py`-prefixed wrapper ident for a model type ident (e.g. `Catalog` →
/// `PyCatalog`). Wrapper names use the parent-qualified emitted type name, which
/// for a top-level `output_type()`/`input_type()` ident is just the simple name,
/// so prefixing is sufficient here.
fn wrapper_ident(model_ident: &Ident) -> Ident {
    format_ident!("Py{}", model_ident)
}

/// The `Py`-prefixed wrapper ident for a (possibly nested) model type, using the
/// **parent-qualified** emitted name so a message-nested type resolves to the same
/// ident the wrapper is defined under (e.g. the nested `…Request.Operation` enum →
/// `PyGenerate…RequestOperation`, not the colliding bare `PyOperation`). For a
/// top-level type this is identical to [`wrapper_ident`] of the simple name.
fn qualified_wrapper_ident(ty: &UnifiedType) -> Ident {
    match &ty.base_type {
        BaseType::Message(name) | BaseType::Enum(name) | BaseType::OneOf(name) => {
            format_ident!("Py{}", crate::utils::extract_qualified_type_name(name))
        }
        _ => wrapper_ident(&ty.type_ident()),
    }
}

/// Wrap a model output ident as its `Py` wrapper return type, and produce the
/// expression converting the bare client result `result` into it.
///
/// Returns `(return_type, convert_expr)`, or `None` for an `Empty`-returning method
/// ([`MethodHandler::output_type`] is `None`). A server-streaming method returns a list.
fn wrap_single_output(method: &MethodHandler<'_>) -> Option<(TokenStream, TokenStream)> {
    if method.is_existence_check() {
        return Some((quote! { bool }, quote! { result }));
    }
    let w = wrapper_ident(&method.output_type()?);
    if method.plan.metadata.server_streaming {
        Some((
            quote! { Vec<#w> },
            quote! { result.into_iter().map(#w::from).collect::<Vec<_>>() },
        ))
    } else {
        Some((quote! { #w }, quote! { #w::from(result) }))
    }
}

/// `mut` keyword for the `let request = ...` binding, but only when there are
/// optional-field setter calls that actually mutate it. Without setters the
/// binding is never reassigned, so `let mut` would trip `unused_mut`.
fn mut_if_setters(builder_calls: &[TokenStream]) -> TokenStream {
    if builder_calls.is_empty() {
        quote! {}
    } else {
        quote! { mut }
    }
}

/// PyO3 binding backend. Holds the configured Python error/result type idents so the method bodies
/// can wrap calls in the right `PyResult`/error conversion.
pub(crate) struct PyBackend<'a> {
    py_error_type: Ident,
    py_result_type: Ident,
    python: &'a PythonOptions,
}

impl<'a> PyBackend<'a> {
    fn from_service(service: &ServiceHandler<'a>) -> Self {
        let bindings = service
            .config
            .bindings
            .as_ref()
            .expect("bindings config required for python output");
        Self {
            py_error_type: format_ident!("{}", bindings.py_error_type),
            py_result_type: format_ident!("{}", bindings.py_result_type),
            python: &bindings.python,
        }
    }

    fn from_services(services: &[ServiceHandler<'a>]) -> Self {
        Self::from_service(services.first().expect("python output has a service"))
    }

    /// The module that defines the Python error and result types: the generated
    /// `crate::codegen::error` with [`PythonOptions::error_module`], else `crate::error`.
    fn error_module(&self) -> TokenStream {
        if self.python.error_module.is_some() {
            quote! { crate::codegen::error }
        } else {
            quote! { crate::error }
        }
    }

    /// The import of the binding crate's runtime hook, `crate::runtime::get_runtime`; none with
    /// the generated runtime ([`PythonOptions::runtime`]).
    fn runtime_import(&self) -> Option<TokenStream> {
        (!self.python.runtime).then(|| quote! { use crate::runtime::get_runtime; })
    }

    /// The expression of the Tokio runtime that a sync method blocks on: the generated
    /// `crate::codegen::runtime::rt()` with [`PythonOptions::runtime`], else the crate's
    /// `get_runtime(py)`.
    fn runtime_call(&self) -> TokenStream {
        if self.python.runtime {
            quote! { crate::codegen::runtime::rt() }
        } else {
            quote! { get_runtime(py)? }
        }
    }

    /// The `register` function that adds every generated class to the extension module.
    ///
    /// With [`PythonOptions::module`], the root client goes in the module itself and the
    /// classes of each proto package in a submodule named after its service segment, which is
    /// also put in `sys.modules` so that `import <module>.<service>` finds it: the generated
    /// `register_submodule!` macro does both, once per submodule. Without it, every
    /// class goes in the module. With the generated runtime ([`PythonOptions::runtime`]) and
    /// `_async` methods, it first installs that runtime in `pyo3-async-runtimes`.
    fn register_fn(&self, services: &[&ServiceHandler<'_>], aggregate: &Ident) -> TokenStream {
        let Some(first) = services.first() else {
            return quote! {};
        };
        // Classes per service segment: the model wrappers, then the scoped clients.
        let mut groups: BTreeMap<&str, Vec<Ident>> = BTreeMap::new();
        let plans: Vec<_> = services.iter().map(|s| s.plan).collect();
        let classes =
            super::model_conversions::ModelClasses::new(&plans, first.metadata, self.python);
        for (segment, wrapper) in classes.pyclasses(self.python) {
            groups.entry(segment).or_default().push(wrapper);
        }
        for service in services.iter().filter(|s| s.emits_scoped_binding()) {
            groups
                .entry(super::package_segment(&service.plan.package))
                .or_default()
                .push(format_ident!("Py{}", service.client_type().to_string()));
        }

        // With a module path, each submodule is registered by `register_submodule!`, which goes
        // before `register`, as a macro is defined before its use.
        let (helpers, body) = match &self.python.module {
            Some(root) => {
                let children = groups.iter().map(|(segment, classes)| {
                    quote! {
                        register_submodule!(module, #segment, [#(#classes),*])?;
                    }
                });
                let helpers = quote! {
                    /// The extension module, as Python imports it.
                    pub const CORE_MODULE: &str = #root;

                    /// Register the submodule `<CORE_MODULE>.<name>` with its classes. It also
                    /// goes in `sys.modules`, so `import <CORE_MODULE>.<name>` finds it.
                    macro_rules! register_submodule {
                        ($parent:expr, $mod_name:expr, [$($type:ty),* $(,)?]) => {{
                            let py = $parent.py();
                            let child = ::pyo3::types::PyModule::new(py, $mod_name)?;
                            $(
                                child.add_class::<$type>()?;
                            )*
                            $parent.add_submodule(&child)?;
                            py.import("sys")?
                                .getattr("modules")?
                                .set_item(format!("{}.{}", CORE_MODULE, $mod_name), child)
                        }};
                    }
                };
                let body = quote! {
                    module.add_class::<#aggregate>()?;
                    #(#children)*
                };
                (helpers, body)
            }
            None => {
                let classes = groups.values().flatten();
                let body = quote! {
                    module.add_class::<#aggregate>()?;
                    #(module.add_class::<#classes>()?;)*
                };
                (quote! {}, body)
            }
        };
        // With the generated runtime, the `_async` methods run on it too.
        let init_async = (self.python.runtime && self.python.async_methods)
            .then(|| quote! { crate::codegen::runtime::init_async_runtime(); });
        let exceptions = self
            .python
            .error_module
            .is_some()
            .then(|| quote! { crate::codegen::error::add_exceptions(module)?; });
        quote! {
            #helpers

            /// Add the generated classes to the extension module `module`.
            pub fn register(module: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
                #init_async
                #exceptions
                #body
                Ok(())
            }
        }
    }
}

pub fn main_module(services: &[ServiceHandler<'_>]) -> crate::error::Result<String> {
    PyBackend::from_services(services).main_module(services)
}

pub(crate) fn generate(service: &ServiceHandler<'_>) -> crate::error::Result<String> {
    PyBackend::from_service(service).generate_service(service)
}

impl BindingBackend for PyBackend<'_> {
    fn generate_service(&self, service: &ServiceHandler<'_>) -> crate::error::Result<String> {
        let bindings = service
            .config
            .bindings
            .as_ref()
            .expect("bindings config required for python output");

        let rust_client_ident = service.client_type();
        let client_ident = format_ident!("Py{}", rust_client_ident);
        let rust_client_name = rust_client_ident.to_string();

        let client_crate = format_ident!("{}", bindings.client_crate_name);
        let py_error_type = &self.py_error_type;
        let py_result_type = &self.py_result_type;

        let methods = driver::scoped_client_methods(self, service);
        let mod_path = service.models_path();
        // The `Py*` model wrappers are re-exported at the models root; import them so
        // the wrapper return/param types resolve.
        let wrappers_import = service
            .models_root_path()
            .map(|root| quote! { use #root::*; });

        let error_module = self.error_module();
        let stub_class = super::stub_class_attr(self.python);
        let stub_methods = super::stub_methods_attr(self.python);
        let module_arg = super::module_arg(
            self.python,
            Some(super::package_segment(&service.plan.package)),
        );
        let futures_import = self
            .python
            .async_methods
            .then(|| quote! { use futures::TryStreamExt; });
        let runtime_import = self.runtime_import();
        let class_doc = self.python.stub_gen.then(|| {
            let resource = service
                .resource()
                .map(|r| r.type_name.clone())
                .unwrap_or_else(|| rust_client_name.clone());
            let doc = format!(" The requests on one `{resource}`.");
            quote! { #[doc = #doc] }
        });

        let tokens = quote! {
            use std::collections::HashMap;
            use pyo3::prelude::*;
            use #client_crate::#rust_client_ident;
            use #mod_path::*;
            #wrappers_import
            use #error_module::{#py_error_type, #py_result_type};
            #runtime_import
            #futures_import

            #class_doc
            #stub_class
            #[pyclass(name = #rust_client_name, #module_arg)]
            pub struct #client_ident {
                pub(crate) client: #rust_client_ident,
            }

            #stub_methods
            #[pymethods]
            impl #client_ident {
                #(#methods)*
            }

            impl #client_ident {
                pub fn new(client: #rust_client_ident) -> Self {
                    Self { client }
                }
            }
        };

        format_tokens(tokens)
    }

    fn main_module(&self, services: &[ServiceHandler<'_>]) -> crate::error::Result<String> {
        // Only resource-scoped services have a per-service scoped module; resource-less services'
        // methods live on the root client.
        // Generated PyO3 modules structurally trip dead_code (full surface
        // emitted), too_many_arguments (flat `#[pymethods]`), and unused_imports
        // (per-module use-prelude). Allow them so the binding crate builds clean.
        let service_modules = services
            .iter()
            .filter(|s| s.emits_scoped_binding())
            .map(|s| {
                let module_name = format_ident!("{}", s.plan.base_path);
                quote! {
                    #[allow(dead_code, unused_imports, clippy::too_many_arguments)]
                    pub mod #module_name;
                }
            });

        let bindings = services
            .first()
            .and_then(|s| s.config.bindings.as_ref())
            .expect("bindings config required for python output");

        let aggregate_client_name = &bindings.aggregate_client_name;
        let client_crate = format_ident!("{}", bindings.client_crate_name);
        let aggregate_client_ident = format_ident!("{}", aggregate_client_name);
        let py_aggregate_client_ident = format_ident!("Py{}", aggregate_client_name);
        let py_error_type = &self.py_error_type;
        let py_result_type = &self.py_result_type;

        let mut sorted_services = services.iter().collect_vec();
        sorted_services.sort_by(|a, b| a.plan.service_name.cmp(&b.plan.service_name));

        // Services can share a package, so dedupe the model imports.
        let mod_paths = glob_imports(sorted_services.iter().map(|s| s.models_path()));
        // Import the `Py*` model wrappers (re-exported at the models root). All
        // services share one root, so dedupe to a single glob import.
        let wrapper_imports =
            glob_imports(sorted_services.iter().filter_map(|s| s.models_root_path()));

        // Only services with a scoped binding expose a per-service scoped client to import.
        let codegen_imports = sorted_services
            .iter()
            .filter(|s| s.emits_scoped_binding())
            .map(|s| {
                let mod_name = format_ident!("{}", s.plan.base_path);
                let client_name = format_ident!("Py{}", s.client_type().to_string());
                quote! { use crate::codegen::#mod_name::#client_name; }
            });

        let methods = driver::root_client_methods(self, &sorted_services);
        let resource_accessor_methods = driver::resource_accessor_methods(self, &sorted_services);

        let error_module = self.error_module();
        let stub_class = super::stub_class_attr(self.python);
        let stub_methods = super::stub_methods_attr(self.python);
        let root_module_arg = super::module_arg(self.python, None);
        let runtime_import = self.runtime_import();
        let helper_modules = {
            let error = self
                .python
                .error_module
                .is_some()
                .then(|| quote! { pub mod error; });
            let future = self
                .python
                .async_methods
                .then(|| quote! { pub mod py_future; });
            let runtime = self.python.runtime.then(|| quote! { pub mod runtime; });
            quote! { #error #future #runtime }
        };
        let class_doc = self.python.stub_gen.then(|| {
            quote! {
                /// A client of the API at one base URL.
            }
        });
        let new_doc = self.python.stub_gen.then(|| {
            quote! {
                /// A client of the API at `base_url`. `token`, when set, goes in every
                /// request as the bearer token.
            }
        });
        let register = self.register_fn(&sorted_services, &py_aggregate_client_ident);

        let tokens = quote! {
            // The aggregate module file carries the root `#[pyclass]` (flat methods →
            // too_many_arguments) and a use-prelude that not every build consumes.
            #![allow(dead_code, unused_imports, clippy::too_many_arguments)]

            #(#service_modules)*
            #helper_modules

            use std::collections::HashMap;
            use futures::stream::TryStreamExt;
            use pyo3::prelude::*;
            use #client_crate::{#aggregate_client_ident};
            use #error_module::{#py_error_type, #py_result_type};
            #runtime_import
            #(#mod_paths)*
            #(#wrapper_imports)*
            #(#codegen_imports)*

            #class_doc
            #stub_class
            #[pyclass(name = #aggregate_client_name, #root_module_arg)]
            pub struct #py_aggregate_client_ident {
                // `pub(crate)`: hand-written `#[pymethods]` blocks of the binding crate can add
                // methods (pyo3's `multiple-pymethods`).
                pub(crate) client: #aggregate_client_ident
            }

            #stub_methods
            #[pymethods]
            impl #py_aggregate_client_ident {
                #new_doc
                #[new]
                #[pyo3(signature = (base_url, token = None))]
                pub fn new(base_url: String, token: Option<String>) -> PyResult<Self> {
                    let base_url = base_url.parse().map_err(#py_error_type::from)?;
                    let client = match token {
                        Some(token) => #aggregate_client_ident::new_with_token(base_url, token),
                        None => #aggregate_client_ident::new_unauthenticated(base_url),
                    };
                    Ok(Self { client })
                }

                #(#methods)*

                #(#resource_accessor_methods)*
            }

            #register
        };

        format_tokens(tokens)
    }

    fn param_defs(&self, method: &MethodHandler<'_>, mode: BindingMode) -> Vec<TokenStream> {
        python_param_defs(method, mode)
    }

    fn client_call(&self, method: &MethodHandler<'_>, mode: BindingMode) -> TokenStream {
        let (method_name, drop_path) = match mode {
            BindingMode::Scoped => (method.plan.resource_client_method(), true),
            BindingMode::Flat => (method.plan.base_method_ident(), false),
        };
        let args = method
            .required_parameters()
            .filter(|param| !(drop_path && param.is_path_param()))
            .map(|param| {
                let param_name = param.field_ident();
                // Required message/enum params arrive as their `Py` wrapper and must
                // be converted back to the bare model the Rust client takes.
                convert_param_value(param.field_type(), quote! { #param_name })
            });
        quote! {
            self.client.#method_name(#(#args,)*)
        }
    }

    fn emit_list(&self, parts: &ShapeParts<'_>) -> TokenStream {
        let method = parts.method;
        let items_field = method
            .list_output_field()
            .expect("List-shaped method must have a list output field");
        // The list item type, and the wrapper it crosses the boundary as. List
        // items are always message types, so `type_ident()` is the model name.
        let item_ident = items_field.unified_type.type_ident();
        let item_wrapper = wrapper_ident(&item_ident);
        self.methods(
            parts,
            method.binding_method_name(),
            true,
            MethodOutput {
                future: quote! {
                    async move { request.into_stream().try_collect::<::std::vec::Vec<_>>().await }
                },
                value: Some((
                    quote! { ::std::vec::Vec<#item_wrapper> },
                    quote! {
                        result.into_iter().map(#item_wrapper::from).collect::<::std::vec::Vec<_>>()
                    },
                )),
            },
        )
    }

    fn emit_create(&self, parts: &ShapeParts<'_>) -> TokenStream {
        let method = parts.method;
        self.single(
            parts,
            method.binding_method_name(),
            wrap_single_output(method),
        )
    }

    fn emit_get_update(&self, parts: &ShapeParts<'_>) -> TokenStream {
        let method = parts.method;
        self.single(parts, scoped_method_name(parts), wrap_single_output(method))
    }

    fn emit_delete(&self, parts: &ShapeParts<'_>) -> TokenStream {
        self.single(parts, scoped_method_name(parts), None)
    }

    fn emit_resource_accessor(&self, service: &ServiceHandler<'_>) -> Option<TokenStream> {
        let spec = service.accessor_spec()?;
        let method_name = format_ident!("{}", spec.singular);
        let client_name = format_ident!("Py{}", service.client_type().to_string());
        let doc = self.python.stub_gen.then(|| {
            let resource = service
                .resource()
                .map(|r| r.type_name.clone())
                .unwrap_or_else(|| spec.singular.clone());
            let text = format!(" The requests on one `{resource}`. It sends no request itself.");
            quote! { #[doc = #text] }
        });

        let param_idents: Vec<_> = spec.params.iter().map(|p| format_ident!("{}", p)).collect();
        let param_list = param_idents
            .iter()
            .map(|id| quote! { #id: String })
            .collect::<Vec<_>>();

        // A nested resource has a composite `full_name`; the underlying Rust client accessor takes a
        // single `full_name` string, so join the decomposed Python params back together with `.`.
        let method_call = if spec.nested {
            let from_full_name_method = format_ident!("{}_from_full_name", spec.singular);
            let format_str = spec.join_format();
            quote! {
                #doc
                pub fn #method_name(&self, #(#param_list),*) -> #client_name {
                    let full_name = format!(#format_str, #(#param_idents),*);
                    #client_name {
                        client: self.client.#from_full_name_method(full_name),
                    }
                }
            }
        } else {
            let param_refs = param_idents
                .iter()
                .map(|id| quote! { #id })
                .collect::<Vec<_>>();
            quote! {
                #doc
                pub fn #method_name(&self, #(#param_list),*) -> #client_name {
                    #client_name {
                        client: self.client.#method_name(#(#param_refs),*),
                    }
                }
            }
        };

        Some(method_call)
    }
}

/// The name of a get, update or delete binding method: the resource client method on a scoped
/// client.
fn scoped_method_name(parts: &ShapeParts<'_>) -> Ident {
    match parts.mode {
        BindingMode::Scoped => parts.method.plan.resource_client_method(),
        BindingMode::Flat => parts.method.binding_method_name(),
    }
}

/// `use <path>::*;` for each of `paths`, once each, in path order.
fn glob_imports(paths: impl Iterator<Item = syn::Path>) -> Vec<TokenStream> {
    paths
        .map(|path| (quote! { #path }.to_string(), path))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .map(|path| quote! { use #path::*; })
        .collect()
}

/// What a binding method gives back: the future of the bare client result (an expression over
/// the `request` builder), and its Python-visible type and the conversion of that `result`.
/// `value` is `None` for a method that gives back nothing: Python `None`.
struct MethodOutput {
    future: TokenStream,
    value: Option<(TokenStream, TokenStream)>,
}

impl PyBackend<'_> {
    /// A method that gives back one model, or nothing.
    fn single(
        &self,
        parts: &ShapeParts<'_>,
        method_name: Ident,
        value: Option<(TokenStream, TokenStream)>,
    ) -> TokenStream {
        let output = MethodOutput {
            future: quote! { request.into_future() },
            value,
        };
        self.methods(parts, method_name, false, output)
    }

    /// The sync binding method and, with [`PythonOptions::async_methods`], its `_async` twin.
    ///
    /// [`PythonOptions::async_methods`]: crate::codegen::PythonOptions::async_methods
    fn methods(
        &self,
        parts: &ShapeParts<'_>,
        method_name: Ident,
        is_list: bool,
        output: MethodOutput,
    ) -> TokenStream {
        let method = parts.method;
        let python = self.python;
        let py_error_type = &self.py_error_type;
        let py_result_type = &self.py_result_type;
        let param_defs = &parts.param_defs;
        let signature = pyo3_signature(method, parts.mode, python.keyword_only);
        let client_call = &parts.client_call;
        let builder_calls = generate_builder_pattern(method, is_list, self);
        let mut_kw = mut_if_setters(&builder_calls);
        let MethodOutput { future, value } = output;
        let (py_type, convert) = match &value {
            Some((py_type, convert)) => (py_type.clone(), convert.clone()),
            None => (quote! { () }, quote! { result }),
        };
        let docs = python.stub_gen.then(|| method_doc_attrs(method));
        let runtime_call = self.runtime_call();

        let sync = quote! {
            #docs
            #signature
            pub fn #method_name(
                &self,
                py: Python,
                #(#param_defs,)*
            ) -> #py_result_type<#py_type> {
                let #mut_kw request = #client_call;
                #(#builder_calls)*
                let runtime = #runtime_call;
                py.detach(|| {
                    // `result` is the bare client value; `#convert` lifts it into the `Py`
                    // wrapper (or passes a unit through unchanged).
                    #[allow(clippy::let_unit_value)]
                    let result = runtime.block_on(#future)?;
                    Ok::<_, #py_error_type>(#convert)
                })
            }
        };
        if !python.async_methods {
            return sync;
        }

        let async_name = format_ident!("{}_async", method_name);
        let async_doc = format!(
            " Like `{method_name}`, but returns an `asyncio.Future` of the result. Call it in a \
             running event loop."
        );
        let (async_type, async_convert) = value.unwrap_or_else(|| {
            (
                quote! { crate::codegen::py_future::NoResult },
                quote! { crate::codegen::py_future::NoResult },
            )
        });
        quote! {
            #sync

            #[doc = #async_doc]
            #signature
            pub fn #async_name<'py>(
                &self,
                py: Python<'py>,
                #(#param_defs,)*
            ) -> ::pyo3::PyResult<crate::codegen::py_future::PyFuture<'py, #async_type>> {
                let #mut_kw request = #client_call;
                #(#builder_calls)*
                crate::codegen::py_future::PyFuture::new(py, async move {
                    #[allow(clippy::let_unit_value, unused_variables)]
                    let result = #future
                        .await
                        .map_err(|e| ::pyo3::PyErr::from(#py_error_type::from(e)))?;
                    Ok(#async_convert)
                })
            }
        }
    }
}

/// `#[doc]` attributes for a binding method: the proto comment, then `Args:` and `Returns:`
/// sections, as in the `.pyi` typings.
fn method_doc_attrs(method: &MethodHandler<'_>) -> TokenStream {
    let doc = super::typings::format_method_docstring_with_params(method);
    if doc.trim().is_empty() {
        return quote! {};
    }
    let lines = doc.lines().map(|l| format!(" {l}"));
    quote! { #(#[doc = #lines])* }
}

impl SetterRender for PyBackend<'_> {
    fn supports(&self, _setter: &OptionalSetter<'_>) -> bool {
        // PyO3 accepts every parameter type; no capability filter.
        true
    }

    fn render_setter(
        &self,
        setter: &OptionalSetter<'_>,
        param_ident: &Ident,
        with_method: &Ident,
    ) -> TokenStream {
        // A free-form JSON argument is any Python object: convert it to the field's value.
        if let Some(json) = json_param(setter.field_type) {
            let value = super::model_conversions::json_from_py(
                json,
                setter.field_type,
                quote! { #param_ident },
                true,
            );
            let value = if setter.field_type.is_repeated
                || matches!(setter.field_type.base_type, BaseType::Map(..))
            {
                value
            } else {
                // The builder takes the bare value, not the `MessageField`.
                let from_py = format_ident!("json_{}_from_py", json.fn_suffix());
                quote! { #from_py(&#param_ident)? }
            };
            return quote! {
                if let Some(#param_ident) = #param_ident {
                    request = request.#with_method(#value);
                }
            };
        }
        // Maps and repeated fields are `Option`-wrapped at the binding boundary; guard before
        // forwarding. Repeated message/enum elements are also converted from their `Py`
        // wrapper to the bare model.
        if matches!(setter.field_type.base_type, BaseType::Map(_, _))
            || setter.field_type.is_repeated
        {
            let inner = if message_map_value(setter.field_type).is_some() {
                convert_map_values(quote! { #param_ident })
            } else if param_needs_conversion(setter.field_type) {
                quote! { #param_ident.into_iter().map(::core::convert::Into::into).collect::<::std::vec::Vec<_>>() }
            } else {
                quote! { #param_ident }
            };
            quote! {
                if let Some(#param_ident) = #param_ident {
                    request = request.#with_method(#inner);
                }
            }
        } else if param_needs_conversion(setter.field_type) {
            // Optional singular message/enum: `Option<PyWrapper>` → `Option<Inner>` for
            // the builder, which accepts `impl Into<Option<Inner>>`.
            quote! {
                request = request.#with_method(#param_ident.map(::core::convert::Into::into));
            }
        } else {
            quote! {
                request = request.#with_method(#param_ident);
            }
        }
    }
}

/// Build the Python method signature param defs from the shared [`MethodHandler::param_plan`], so
/// path/`page_token` filtering lives in one place.
///
/// Message- and enum-typed params cross the boundary as their `Py` wrapper type
/// (e.g. `Option<Catalog>` → `Option<PyCatalog>`); scalars are unchanged. The
/// matching value conversion happens at the call/setter site
/// ([`PyBackend::client_call`] / [`PyBackend::render_setter`]).
fn python_param_defs(method: &MethodHandler<'_>, mode: BindingMode) -> Vec<TokenStream> {
    method
        .param_plan(mode)
        .into_iter()
        .map(|p| {
            let param_name = p.field_ident();
            if json_param(p.field_type()).is_some() {
                // Any Python object; `None` for an optional argument left out.
                return if p.is_optional() {
                    quote! { #param_name: ::core::option::Option<::pyo3::Bound<'_, ::pyo3::PyAny>> }
                } else {
                    quote! { #param_name: ::pyo3::Bound<'_, ::pyo3::PyAny> }
                };
            }
            let rust_type = method.field_type(p.field_type(), RenderContext::PythonParameter);
            let py_type = wrap_param_type(p.field_type(), &rust_type);
            // Every list and map is an `Option` at the boundary; a required one is not.
            let py_type = if p.is_optional() || !is_collection(p.field_type()) {
                py_type
            } else {
                strip_option(py_type)
            };
            quote! { #param_name: #py_type }
        })
        .collect()
}

/// The free-form JSON type of a param (`google.protobuf.Value`, `Struct`, `ListValue`), which
/// Python passes as a plain object.
fn json_param(ty: &UnifiedType) -> Option<JsonType> {
    JsonType::of_field(ty)
}

/// Whether a param's base type is a model (message/enum) that has a `Py` wrapper,
/// so its value must be converted (`.into()`) when forwarded to the Rust client.
/// The value type of a `map<K, Message>` param, whose values cross the boundary as wrappers.
fn message_map_value(ty: &UnifiedType) -> Option<&UnifiedType> {
    match &ty.base_type {
        BaseType::Map(_, value) if matches!(value.base_type, BaseType::Message(_)) => Some(value),
        _ => None,
    }
}

/// The expression that converts the wrapper values of a map param `expr` to the models: each
/// wrapper holds its model in `.0`.
fn convert_map_values(expr: TokenStream) -> TokenStream {
    quote! {
        #expr
            .into_iter()
            .map(|(k, v)| (k, v.0))
            .collect::<::std::collections::HashMap<_, _>>()
    }
}

fn param_needs_conversion(ty: &UnifiedType) -> bool {
    matches!(
        ty.base_type,
        BaseType::Message(_) | BaseType::Enum(_) | BaseType::OneOf(_)
    )
}

/// Rewrite a rendered param `syn::Type` so a message/enum base type becomes its
/// `Py` wrapper, preserving the `Option`/`Vec` cardinality wrapping. Scalars,
/// maps, and `()` are returned unchanged.
fn wrap_param_type(ty: &UnifiedType, rendered: &syn::Type) -> TokenStream {
    if let Some(value) = message_map_value(ty) {
        let wrapper = qualified_wrapper_ident(value);
        return quote! {
            ::core::option::Option<::std::collections::HashMap<::std::string::String, #wrapper>>
        };
    }
    if !param_needs_conversion(ty) {
        return quote! { #rendered };
    }
    let wrapper = qualified_wrapper_ident(ty);
    let mut inner = quote! { #wrapper };
    if ty.is_repeated {
        inner = quote! { ::std::vec::Vec<#inner> };
    }
    // FFI rule (`should_wrap_in_option`): optional fields and FFI collections are
    // `Option`-wrapped. Repeated message/enum params follow the same rule as the
    // scalar renderer, so mirror `is_optional || is_repeated` here.
    if ty.is_optional || ty.is_repeated {
        inner = quote! { ::core::option::Option<#inner> };
    }
    inner
}

/// Whether a param is a list or a map.
fn is_collection(ty: &UnifiedType) -> bool {
    ty.is_repeated || matches!(ty.base_type, BaseType::Map(..))
}

/// `T` for a rendered `Option<T>`; any other type unchanged.
fn strip_option(ty: TokenStream) -> TokenStream {
    let Ok(syn::Type::Path(path)) = syn::parse2::<syn::Type>(ty.clone()) else {
        return ty;
    };
    let Some(last) = path.path.segments.last() else {
        return ty;
    };
    match &last.arguments {
        syn::PathArguments::AngleBracketed(args) if last.ident == "Option" => {
            match args.args.first() {
                Some(syn::GenericArgument::Type(inner)) => quote! { #inner },
                _ => ty,
            }
        }
        _ => ty,
    }
}

/// The expression that converts the Python-wrapper value `expr` of a required param back into
/// the bare model the Rust client expects. A required list is a plain `Vec` (see
/// [`python_param_defs`]). Scalars pass through.
fn convert_param_value(ty: &UnifiedType, expr: TokenStream) -> TokenStream {
    if let Some(json) = json_param(ty) {
        if ty.is_repeated || matches!(ty.base_type, BaseType::Map(..)) {
            return super::model_conversions::json_from_py(json, ty, expr, true);
        }
        let from_py = format_ident!("json_{}_from_py", json.fn_suffix());
        return quote! { #from_py(&#expr)? };
    }
    if message_map_value(ty).is_some() {
        return convert_map_values(expr);
    }
    if !param_needs_conversion(ty) {
        return expr;
    }
    match (ty.is_repeated, ty.is_optional) {
        (true, _) => quote! {
            #expr.into_iter().map(::core::convert::Into::into).collect()
        },
        (false, true) => quote! { #expr.map(::core::convert::Into::into) },
        (false, false) => quote! { #expr.into() },
    }
}

/// The `#[pyo3(signature = (...))]` attribute for a method, derived from its param plan.
/// With `keyword_only`, a `*` goes before the first optional parameter.
fn pyo3_signature(
    method: &MethodHandler<'_>,
    mode: BindingMode,
    keyword_only: bool,
) -> TokenStream {
    let mut signature_parts = Vec::new();
    // The `*` that goes before the first optional parameter.
    let mut star = keyword_only;
    for p in method.param_plan(mode).iter() {
        let ident = p.field_ident();
        if p.is_optional() {
            if std::mem::take(&mut star) {
                signature_parts.push(quote! { * });
            }
            signature_parts.push(quote! { #ident = None });
        } else {
            signature_parts.push(quote! { #ident });
        }
    }
    if signature_parts.is_empty() {
        quote! {}
    } else {
        quote! {
            #[pyo3(signature = (#(#signature_parts),*))]
        }
    }
}
