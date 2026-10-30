//! The generated Python bindings, mounted under `crate::codegen` as a binding crate mounts them.

include!(concat!(env!("OUT_DIR"), "/python_mod.rs"));

/// The directory of the generated Python package `demo`.
pub const PACKAGE_DIR: &str = concat!(env!("OUT_DIR"), "/pkg");

/// A token source made in Rust, as a binding crate hands one to `token_provider=`: the
/// tokens `t1`, `t2`, … at each request.
#[cfg(test)]
#[pyo3::pyfunction]
fn counting_source() -> codegen::RustTokenSource {
    use rest_client_e2e::transport::{TokenFuture, TokenSource};

    #[derive(Debug, Default)]
    struct Counting(std::sync::atomic::AtomicUsize);

    impl TokenSource for Counting {
        fn token(&self) -> TokenFuture<'_> {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            Box::pin(async move { Ok(format!("t{n}")) })
        }
    }

    codegen::RustTokenSource {
        source: std::sync::Arc::new(Counting::default()),
    }
}

#[cfg(test)]
mod tests;
