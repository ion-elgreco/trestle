//! The generated models (`crate::models`) and REST client (`crate::codegen`) of
//! `proto/demo/v1/svc.proto`, mounted as a client crate mounts them.

// The generated code refers to this crate by name.
extern crate self as rest_client_e2e;

pub mod models {
    include!(concat!(env!("OUT_DIR"), "/models_mod.rs"));
    pub use _gen::*;
}

pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/client/mod.rs"));
}

// The binding crate imports the clients and the error module from the crate root.
pub use codegen::*;

/// Context type of the (unused) server config.
pub struct Context;

/// The token source of a `DemoClient(url)` without a token: the host `localhost` gets the
/// token `defaulted`, `fail.invalid` fails, any other host gets none.
pub async fn default_token_source(
    base_url: &url::Url,
) -> Result<Option<std::sync::Arc<dyn transport::TokenSource>>, transport::TokenError> {
    match base_url.host_str() {
        Some("localhost") => Ok(Some(std::sync::Arc::new(transport::StaticToken(
            "defaulted".into(),
        )))),
        Some("fail.invalid") => Err("no default token".into()),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests;
