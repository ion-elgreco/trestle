//! The plain `reqwest` transport for generated REST clients.
//!
//! Selected with `transport_type_path =` [`REQWEST_TRANSPORT_TYPE_PATH`]. The generator then emits
//! `transport.rs` into the client output: an `HttpClient` that wraps a `reqwest::Client`, an
//! optional bearer token and an optional `User-Agent`, with the per-verb builder methods the
//! client bodies call. It has no cloud credential providers and no retries, and it builds for
//! native targets and `wasm32` alike (reqwest uses the browser `fetch` there).

use quote::quote;

use super::format_tokens_static;

/// Transport type path that selects the generated `reqwest` transport.
pub const REQWEST_TRANSPORT_TYPE_PATH: &str = "crate::codegen::transport::HttpClient";

/// The `transport.rs` file of the client output. With `user_agent`, the default transport sends
/// `<user_agent>/<version of the client crate>` as its `User-Agent`, except in the browser. With
/// `connect`, the transport sends ConnectRPC requests too (`connectrpc::client::ClientTransport`),
/// with the same token and `User-Agent`, outside the browser.
pub(crate) fn generate(
    user_agent: Option<&str>,
    connect: Option<&super::RuntimeConnect>,
) -> String {
    let connect = connect.map(|runtime| {
        let cfg = super::client::connect_cfg(runtime.feature.as_deref());
        quote! {
            /// What a ConnectRPC request through the transport fails with, before an answer.
            #cfg
            #[derive(Debug)]
            pub enum ConnectTransportError {
                /// The request body could not be read.
                Body(::std::boxed::Box<dyn ::std::error::Error + Send + Sync>),
                /// The token or the `User-Agent` is not a header value.
                Header(::http::header::InvalidHeaderValue),
                /// The request could not be built or sent.
                Http(reqwest::Error),
            }

            #cfg
            impl ::std::fmt::Display for ConnectTransportError {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        Self::Body(e) => write!(f, "request body: {e}"),
                        Self::Header(e) => write!(f, "request header: {e}"),
                        Self::Http(e) => write!(f, "request failed: {e}"),
                    }
                }
            }

            #cfg
            impl ::std::error::Error for ConnectTransportError {
                fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
                    match self {
                        Self::Body(e) => Some(e.as_ref()),
                        Self::Header(e) => Some(e),
                        Self::Http(e) => Some(e),
                    }
                }
            }

            #cfg
            impl ::connectrpc::client::ClientTransport for HttpClient {
                type ResponseBody = reqwest::Body;
                type Error = ConnectTransportError;

                fn send(
                    &self,
                    request: ::http::Request<::connectrpc::client::ClientBody>,
                ) -> ::connectrpc::client::BoxFuture<
                    'static,
                    ::std::result::Result<::http::Response<Self::ResponseBody>, Self::Error>,
                > {
                    let transport = self.clone();
                    Box::pin(async move {
                        let (parts, body) = request.into_parts();
                        // The body of a unary call is whole already; it comes as a stream.
                        let bytes = ::http_body_util::BodyExt::collect(body)
                            .await
                            .map_err(|e| ConnectTransportError::Body(e.into()))?
                            .to_bytes();
                        let mut request =
                            reqwest::Request::try_from(::http::Request::from_parts(parts, bytes))
                                .map_err(ConnectTransportError::Http)?;
                        if let Some(user_agent) = &transport.user_agent {
                            let value = user_agent.parse().map_err(ConnectTransportError::Header)?;
                            request.headers_mut().insert(reqwest::header::USER_AGENT, value);
                        }
                        if let Some(token) = &transport.token {
                            let value = format!("Bearer {token}")
                                .parse()
                                .map_err(ConnectTransportError::Header)?;
                            request.headers_mut().insert(reqwest::header::AUTHORIZATION, value);
                        }
                        let response = transport
                            .client
                            .execute(request)
                            .await
                            .map_err(ConnectTransportError::Http)?;
                        Ok(::http::Response::from(response))
                    })
                }
            }
        }
    });
    let (derive, default) = match user_agent {
        None => (quote! { #[derive(Clone, Default)] }, quote! {}),
        Some(product) => {
            let prefix = format!("{product}/");
            let doc =
                format!(" own, its `User-Agent` is `{product}/` and the version of this crate.");
            let default = quote! {
                impl Default for HttpClient {
                    /// A transport with a default `reqwest::Client`. Outside the browser, which sends its
                    #[doc = #doc]
                    fn default() -> Self {
                        let client = Self::new(reqwest::Client::default());
                        #[cfg(not(target_arch = "wasm32"))]
                        let client =
                            client.with_user_agent(concat!(#prefix, env!("CARGO_PKG_VERSION")));
                        client
                    }
                }
            };
            (quote! { #[derive(Clone)] }, default)
        }
    };
    let tokens = quote! {
        use reqwest::{Method, RequestBuilder};
        use url::Url;

        /// The HTTP transport of the generated clients: a `reqwest::Client`, plus a bearer token
        /// and a `User-Agent` that go in every request when set.
        #derive
        pub struct HttpClient {
            client: reqwest::Client,
            token: Option<String>,
            user_agent: Option<String>,
        }

        #default

        // Hand-written so that the token never shows up in logs or panic messages.
        impl ::std::fmt::Debug for HttpClient {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_struct("HttpClient")
                    .field("token", &self.token.as_ref().map(|_| "<redacted>"))
                    .field("user_agent", &self.user_agent)
                    .finish_non_exhaustive()
            }
        }

        impl HttpClient {
            /// A transport that sends its requests with `client`, without a token or a
            /// `User-Agent` of its own.
            pub fn new(client: reqwest::Client) -> Self {
                Self { client, token: None, user_agent: None }
            }

            /// The same transport, sending `token` as the bearer token of every request.
            pub fn with_token(mut self, token: impl Into<String>) -> Self {
                self.token = Some(token.into());
                self
            }

            /// The same transport, sending `user_agent` as the `User-Agent` of every request.
            pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
                self.user_agent = Some(user_agent.into());
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

            /// Start a `HEAD` request.
            pub fn head(&self, url: Url) -> RequestBuilder {
                self.request(Method::HEAD, url)
            }

            /// Start a request with any method.
            pub fn request(&self, method: Method, url: Url) -> RequestBuilder {
                let mut builder = self.client.request(method, url);
                if let Some(user_agent) = &self.user_agent {
                    builder = builder.header(reqwest::header::USER_AGENT, user_agent);
                }
                match &self.token {
                    Some(token) => builder.bearer_auth(token),
                    None => builder,
                }
            }
        }

        #connect
    };
    format_tokens_static(tokens)
}
