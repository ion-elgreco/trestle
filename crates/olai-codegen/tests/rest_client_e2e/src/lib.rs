//! The generated models and REST client of `proto/demo/v1/svc.proto`, mounted as a client crate
//! mounts them: models under `crate::models`, the client under `crate::codegen`.
//!
//! The tests run the client against a local axum server.

// The generated code names this crate `rest_client_e2e`, as another crate would.
extern crate self as rest_client_e2e;

pub mod models {
    include!(concat!(env!("OUT_DIR"), "/models_mod.rs"));
    pub use _gen::*;
}

pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/client/mod.rs"));
}

// The binding crate names the clients and the error module from the crate root.
// The client root re-exports the root client and every resource client.
pub use codegen::*;

/// Context type of the (unused) server config.
pub struct Context;

#[cfg(test)]
mod tests;
