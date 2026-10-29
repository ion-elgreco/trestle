//! The generated Python bindings, mounted as a binding crate mounts them: under `crate::codegen`.

include!(concat!(env!("OUT_DIR"), "/python_mod.rs"));

/// The directory of the generated Python package `demo`.
pub const PACKAGE_DIR: &str = concat!(env!("OUT_DIR"), "/pkg");

#[cfg(test)]
mod tests;
