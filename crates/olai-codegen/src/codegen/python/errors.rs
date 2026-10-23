//! The generated `codegen/error.rs` of the Python bindings ([`PythonOptions::error_module`]).
//!
//! It defines the error and result types that the binding methods return
//! ([`BindingsConfig::py_error_type`] / [`BindingsConfig::py_result_type`]): a client error, or
//! a Python error. A client error becomes an exception of the generated `_errors.py`: the class
//! by the HTTP status of the answer, with its message, code, status and details.
//!
//! [`PythonOptions::error_module`]: crate::codegen::PythonOptions::error_module
//! [`BindingsConfig::py_error_type`]: crate::codegen::BindingsConfig::py_error_type
//! [`BindingsConfig::py_result_type`]: crate::codegen::BindingsConfig::py_result_type

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::package::ERROR_CLASSES;
use crate::codegen::{BindingsConfig, PyPackageConfig, format_tokens_static};

/// The `error.rs` file of the Python bindings.
pub(crate) fn generate(bindings: &BindingsConfig, package: &PyPackageConfig) -> String {
    let error = format_ident!("{}", bindings.py_error_type);
    let result = format_ident!("{}", bindings.py_result_type);
    let client_crate = format_ident!("{}", bindings.client_crate_name);
    let errors_module = format!("{}._errors", package.name);
    let base = &package.error_base_class;
    let error_doc = format!(
        " The error of a binding call: a client error, raised as an exception of \
         `{errors_module}`, or a Python error, raised as is."
    );

    let status_arms: Vec<TokenStream> = ERROR_CLASSES
        .iter()
        .filter(|(_, statuses, _)| !statuses.is_empty())
        .map(|(class, statuses, _)| {
            let statuses = statuses.iter();
            quote! { #(Some(#statuses))|* => #class, }
        })
        .collect();

    let tokens = quote! {
        //! Errors of the Python bindings.

        use pyo3::prelude::*;
        use pyo3::types::{PyDict, PyList, PyString};

        /// The client error type.
        type ClientError = #client_crate::error::Error;

        /// The result of a binding call.
        pub type #result<T> = ::core::result::Result<T, #error>;

        #[doc = #error_doc]
        #[derive(Debug)]
        pub enum #error {
            /// A client error.
            Client(ClientError),
            /// A Python error.
            Python(PyErr),
        }

        impl ::core::convert::From<ClientError> for #error {
            fn from(e: ClientError) -> Self {
                #error::Client(e)
            }
        }

        impl ::core::convert::From<PyErr> for #error {
            fn from(e: PyErr) -> Self {
                #error::Python(e)
            }
        }

        impl ::core::convert::From<::url::ParseError> for #error {
            fn from(e: ::url::ParseError) -> Self {
                #error::Client(ClientError::from(e))
            }
        }

        impl ::core::convert::From<#error> for PyErr {
            fn from(e: #error) -> Self {
                match e {
                    #error::Python(e) => e,
                    #error::Client(e) => {
                        Python::attach(|py| client_exception(py, e).unwrap_or_else(|err| err))
                    }
                }
            }
        }

        /// The exception class of an answer with HTTP status `status`.
        fn class_name(status: Option<u16>) -> &'static str {
            match status {
                #(#status_arms)*
                Some(500..=599) => "ServerError",
                _ => #base,
            }
        }

        /// The exception of a client error.
        fn client_exception(py: Python<'_>, e: ClientError) -> PyResult<PyErr> {
            static ERRORS: ::pyo3::sync::PyOnceLock<Py<::pyo3::types::PyModule>> =
                ::pyo3::sync::PyOnceLock::new();
            let (message, code, status, details) = match e {
                ClientError::Api(api) => {
                    let details = api
                        .details
                        .iter()
                        .map(|d| json_value(py, d))
                        .collect::<PyResult<::std::vec::Vec<_>>>()?;
                    (api.message, api.code, Some(api.status), details)
                }
                other => (other.to_string(), None, None, ::std::vec::Vec::new()),
            };
            let errors = ERRORS.get_or_try_init(py, || py.import(#errors_module).map(Bound::unbind))?;
            let class = errors.bind(py).getattr(class_name(status))?;
            let exception = class.call1((message, code, status, PyList::new(py, details)?))?;
            Ok(PyErr::from_value(exception))
        }

        /// A JSON value as a Python object.
        fn json_value<'py>(py: Python<'py>, value: &::serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
            use ::serde_json::Value;
            Ok(match value {
                Value::Null => py.None().into_bound(py),
                Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
                Value::Number(n) => match (n.as_i64(), n.as_u64()) {
                    (Some(i), _) => i.into_pyobject(py)?.into_any(),
                    (None, Some(u)) => u.into_pyobject(py)?.into_any(),
                    (None, None) => n.as_f64().unwrap_or(f64::NAN).into_pyobject(py)?.into_any(),
                },
                Value::String(s) => PyString::new(py, s).into_any(),
                Value::Array(items) => PyList::new(
                    py,
                    items
                        .iter()
                        .map(|item| json_value(py, item))
                        .collect::<PyResult<::std::vec::Vec<_>>>()?,
                )?
                .into_any(),
                Value::Object(map) => {
                    let dict = PyDict::new(py);
                    for (key, item) in map {
                        dict.set_item(key, json_value(py, item)?)?;
                    }
                    dict.into_any()
                }
            })
        }
    };
    format_tokens_static(tokens)
}
