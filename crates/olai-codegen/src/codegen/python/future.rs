//! The generated `codegen/py_future.rs` of the Python bindings
//! ([`PythonOptions::async_methods`]): what the `_async` methods return.
//!
//! [`PythonOptions::async_methods`]: crate::codegen::PythonOptions::async_methods

use quote::quote;

use crate::codegen::format_tokens_static;

/// The `py_future.rs` file. `PyFuture<T>` is `asyncio.Future[T]` in the stubs.
pub(crate) fn generate() -> String {
    let stub_impls = quote! {
            impl ::pyo3_stub_gen::PyStubType for NoResult {
                fn type_output() -> ::pyo3_stub_gen::TypeInfo {
                    ::pyo3_stub_gen::TypeInfo::none()
                }
            }

            impl<T: ::pyo3_stub_gen::PyStubType> ::pyo3_stub_gen::PyStubType for PyFuture<'_, T> {
                fn type_output() -> ::pyo3_stub_gen::TypeInfo {
                    let result = T::type_output();
                    let mut import = result.import;
                    import.insert("asyncio".into());
                    ::pyo3_stub_gen::TypeInfo {
                        name: ::std::format!("asyncio.Future[{}]", result.name),
                        source_module: None,
                        import,
                        type_refs: result.type_refs,
                    }
                }
            }
    };

    let tokens = quote! {
        //! What the `_async` methods return.

        use std::future::Future;
        use std::marker::PhantomData;

        use pyo3::prelude::*;

        /// The `asyncio.Future` of a `T`, from `future_into_py`.
        pub struct PyFuture<'py, T> {
            future: Bound<'py, PyAny>,
            result: PhantomData<T>,
        }

        impl<'py, T> PyFuture<'py, T>
        where
            T: for<'a> IntoPyObject<'a> + Send + 'static,
        {
            /// Run `future` on the Tokio runtime of `pyo3-async-runtimes`; its result completes
            /// the `asyncio.Future`. Needs a running event loop.
            pub fn new(
                py: Python<'py>,
                future: impl Future<Output = PyResult<T>> + Send + 'static,
            ) -> PyResult<Self> {
                // `future_into_py` skips the runtime's own checks (such as the one for a
                // forked child), so run them first.
                crate::codegen::runtime::rt();
                Ok(Self {
                    future: ::pyo3_async_runtimes::tokio::future_into_py(py, future)?,
                    result: PhantomData,
                })
            }
        }

        impl<'py, T> IntoPyObject<'py> for PyFuture<'py, T> {
            type Target = PyAny;
            type Output = Bound<'py, PyAny>;
            type Error = ::std::convert::Infallible;

            fn into_pyobject(self, _py: Python<'py>) -> ::core::result::Result<Self::Output, Self::Error> {
                Ok(self.future)
            }
        }

        /// Python's `None`, for a request that returns nothing (`()` would be an empty tuple).
        pub struct NoResult;

        impl<'py> IntoPyObject<'py> for NoResult {
            type Target = PyAny;
            type Output = Bound<'py, PyAny>;
            type Error = ::std::convert::Infallible;

            fn into_pyobject(self, py: Python<'py>) -> ::core::result::Result<Self::Output, Self::Error> {
                Ok(py.None().into_bound(py))
            }
        }

        #stub_impls
    };
    format_tokens_static(tokens)
}
