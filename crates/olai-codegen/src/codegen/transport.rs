//! The plain `reqwest` transport for generated REST clients.
//!
//! Selected with `transport_type_path =` [`REQWEST_TRANSPORT_TYPE_PATH`]. The generator then emits
//! `transport.rs` into the client output: an `HttpClient` that wraps a `reqwest::Client` and an
//! optional bearer token, with the per-verb builder methods the client bodies call. It has no
//! cloud credential providers and no retries, and it builds for native targets and `wasm32` alike
//! (reqwest uses the browser `fetch` there).

use quote::quote;

use super::format_tokens_static;

/// Transport type path that selects the generated `reqwest` transport.
pub const REQWEST_TRANSPORT_TYPE_PATH: &str = "crate::codegen::transport::HttpClient";

/// The `transport.rs` file of the client output.
pub(crate) fn generate() -> String {
    let tokens = quote! {
        use reqwest::{Method, RequestBuilder};
        use url::Url;

        /// The HTTP transport of the generated clients: a `reqwest::Client`, plus a bearer token
        /// that goes in every request when set.
        #[derive(Clone, Default)]
        pub struct HttpClient {
            client: reqwest::Client,
            token: Option<String>,
        }

        // Hand-written so that the token never shows up in logs or panic messages.
        impl ::std::fmt::Debug for HttpClient {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_struct("HttpClient")
                    .field("token", &self.token.as_ref().map(|_| "<redacted>"))
                    .finish_non_exhaustive()
            }
        }

        impl HttpClient {
            /// A transport that sends its requests with `client`, without a token.
            pub fn new(client: reqwest::Client) -> Self {
                Self { client, token: None }
            }

            /// The same transport, sending `token` as the bearer token of every request.
            pub fn with_token(mut self, token: impl Into<String>) -> Self {
                self.token = Some(token.into());
                self
            }

            /// Start a `GET` request.
            pub fn get(&self, url: Url) -> RequestBuilder {
                self.request(Method::GET, url)
            }

            /// Start a `POST` request.
            pub fn post(&self, url: Url) -> RequestBuilder {
                self.request(Method::POST, url)
            }

            /// Start a `PUT` request.
            pub fn put(&self, url: Url) -> RequestBuilder {
                self.request(Method::PUT, url)
            }

            /// Start a `PATCH` request.
            pub fn patch(&self, url: Url) -> RequestBuilder {
                self.request(Method::PATCH, url)
            }

            /// Start a `DELETE` request.
            pub fn delete(&self, url: Url) -> RequestBuilder {
                self.request(Method::DELETE, url)
            }

            /// Start a request with any method.
            pub fn request(&self, method: Method, url: Url) -> RequestBuilder {
                let builder = self.client.request(method, url);
                match &self.token {
                    Some(token) => builder.bearer_auth(token),
                    None => builder,
                }
            }
        }
    };
    format_tokens_static(tokens)
}
