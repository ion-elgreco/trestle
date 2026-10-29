//! Low-level HTTP client generation.
//!
//! This is part of the **Generation** stage of the Analysis → Planning → Generation → Output
//! pipeline (see [`super`]). For each service it emits a low-level client struct holding a
//! transport client plus a base [`Url`](url::Url), with one async method per RPC that builds the
//! request URL, serializes the body, and dispatches through the configured transport. The client
//! is written to each service's `client.rs` and is the foundation that the request builders in
//! [`super::builder`] and the ergonomic resource clients in [`super::resource_client`] build upon.
//!
//! The transport type is configurable (`transport_type_path`): the default cloud transport and the
//! WASM browser transport are both selected here. URL routing, parameter sources, and body shape
//! all come from the per-method [`GenerationPlan`](crate::analysis::GenerationPlan) produced during
//! Planning; the emitted token stream is pretty-printed via `super::format_tokens`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{doc_tokens, extract_type_ident, format_tokens};
use crate::Result;
use crate::codegen::{MethodHandler, Runtime, ServiceHandler};
use crate::parsing::MessageField;
use crate::parsing::types::BaseType;

/// Generate client code for a service
pub(crate) fn generate(service: &ServiceHandler<'_>) -> Result<String> {
    let mut method_tokens = Vec::new();
    let connect = connect_side(service);

    for method in service.rest_methods() {
        let full_name = format!(
            "{}.{}.{}",
            service.plan.package, service.plan.service_name, method.plan.metadata.method_name
        );
        let options = &service.config.rest_client;
        let form_body = options.form_encoded_methods.contains(&full_name);
        let path_fields_in_body = options.path_fields_in_body_methods.contains(&full_name);
        let method_code = client_method(method, form_body, path_fields_in_body, connect.as_ref())?;
        method_tokens.push(method_code);
    }

    let client_ident = service.low_level_client_type(crate::codegen::ClientProtocol::Rest);
    let mod_path = service.models_path();
    let foreign_models = service.foreign_models_imports();
    let result_path = service.config.client_result_path();
    let (transport_import, transport_ident) = transport_tokens(service.config);
    // With `connect_at_runtime`, every client carries the protocol of its calls; the services
    // served over Connect too have a Connect client with the same transport.
    let runtime_connect = service.config.rest_client.connect_at_runtime.is_some();
    let protocol_field = runtime_connect.then(|| {
        quote! {
            /// The protocol of the calls; see [`Protocol`](super::super::Protocol).
            pub protocol: super::super::Protocol,
        }
    });
    let protocol_param = runtime_connect.then(|| quote! { , protocol: super::super::Protocol });
    let protocol_init = runtime_connect.then(|| quote! { , protocol });
    let connect_client = connect.as_ref().map(|side| {
        let cfg = &side.cfg;
        let client = &side.client;
        quote! {
            /// The client of the service over ConnectRPC, in binary protobuf, with the same
            /// transport, token and base URL.
            #cfg
            fn connect(&self) -> #client<#transport_ident> {
                let base = self.base_url.as_str().parse().expect("a URL is a URI");
                #client::new(self.client.clone(), ::connectrpc::client::ClientConfig::new(base))
            }
        }
    });
    // `encode_path_value` is in the client `mod.rs`: two levels up, three when each protocol has
    // its own subdirectory.
    let encode_import = service
        .rest_methods()
        .any(|m| m.plan.formats_path_values())
        .then(|| {
            if service.config.client_protocols.is_multi() {
                quote! { use super::super::super::encode_path_value; }
            } else {
                quote! { use super::super::encode_path_value; }
            }
        });

    let tokens = quote! {
        // `use #mod_path::*` is a convenience-wide model glob; a service that
        // references only some model types (common under buffa) would otherwise
        // trip `unused_imports` under `-D warnings`.
        #![allow(unused_imports)]
        #transport_import
        use url::Url;
        use #result_path;
        use #mod_path::*;
        #(#foreign_models)*
        #encode_import

        /// HTTP client for service operations
        #[derive(Clone)]
        pub struct #client_ident {
            pub(crate) client: #transport_ident,
            pub(crate) base_url: Url,
            #protocol_field
        }

        impl #client_ident {
            /// Create a new client instance
            pub fn new(client: #transport_ident, mut base_url: Url #protocol_param) -> Self {
                if !base_url.path().ends_with('/') {
                    base_url.set_path(&format!("{}/", base_url.path()));
                }
                Self { client, base_url #protocol_init }
            }

            #connect_client

            #(#method_tokens)*
        }
    };

    format_tokens(tokens)
}

/// The `#[cfg]` of the Connect side of the clients, with `connect_at_runtime`: the crate's
/// feature when it has one, and never `wasm32`.
pub(crate) fn connect_cfg(feature: Option<&str>) -> TokenStream {
    match feature {
        Some(feature) => quote! { #[cfg(all(feature = #feature, not(target_arch = "wasm32")))] },
        None => quote! { #[cfg(not(target_arch = "wasm32"))] },
    }
}

/// The Connect side of a service's client, with `connect_at_runtime` and the service's
/// `(olai.trestle.v1.connect)`: the `#[cfg]` of its code and the path of connect-rust's client of
/// the service.
pub(crate) struct ConnectSide {
    pub(crate) cfg: TokenStream,
    pub(crate) client: syn::Path,
}

pub(crate) fn connect_side(service: &ServiceHandler<'_>) -> Option<ConnectSide> {
    let runtime = service.config.rest_client.connect_at_runtime.as_ref()?;
    if !service.plan.connect {
        return None;
    }
    let mut client =
        crate::codegen::config::ModelsPath::from_template(&runtime.client_path_template)
            .resolve(&service.models_segment(), &service.models_version());
    client
        .segments
        .push(format_ident!("{}Client", service.plan.service_name).into());
    Some(ConnectSide {
        cfg: connect_cfg(runtime.feature.as_deref()),
        client,
    })
}

/// Whether the method is called over Connect when the client prefers it: a call with a message
/// or a stream of them each way. The others stay REST: a raw body in one message, and the
/// existence checks (`HEAD`).
fn over_connect(method: &MethodHandler<'_>) -> bool {
    let raw = method.plan.raw_request_body.is_some() || method.plan.raw_response_body.is_some();
    method.input_type().is_some()
        && (!raw || method.streams_raw_body())
        && !method.is_existence_check()
}

/// The transport import + the identifier generated client code refers to it by.
///
/// - **Single transport** (default): emit `use <transport_type_path>;` and refer
///   to the transport by its final segment (`CloudClient`), keeping native-only
///   output simple.
/// - **Dual transport** ([`CodeGenConfig::dual_transport`], i.e. WASM output is
///   enabled): emit a `cfg(target_arch = "wasm32")`-gated alias so the one client
///   crate builds natively against `CloudClient` and for the browser against
///   `olai_http_wasm::WasmClient`, and refer to it by the alias `Transport`.
pub(crate) fn transport_tokens(
    config: &crate::codegen::CodeGenConfig,
) -> (TokenStream, proc_macro2::Ident) {
    if config.dual_transport() {
        let import = quote! {
            #[cfg(not(target_arch = "wasm32"))]
            use ::olai_http::CloudClient as Transport;
            #[cfg(target_arch = "wasm32")]
            use ::olai_http_wasm::WasmClient as Transport;
        };
        (import, format_ident!("Transport"))
    } else {
        let transport_path: syn::Path =
            syn::parse_str(&config.transport_type_path).expect("valid transport_type_path");
        let ident = transport_path
            .segments
            .last()
            .expect("transport_type_path has at least one segment")
            .ident
            .clone();
        (quote! { use #transport_path; }, ident)
    }
}

/// Generate client method implementation. `form_body` sends the request body form-encoded
/// instead of as JSON. The body leaves out the fields that go in the URL path, as
/// `google.api.http` maps them, since a strict server rejects fields that it does not know;
/// `path_fields_in_body` keeps them.
///
/// A server-streaming method reads the body as NDJSON, one output message per line. Output fields
/// with a `response_header` come from the response headers; their body may be empty.
///
/// A `raw_body` input field is sent as the request body, as raw bytes; a `raw_body` output field
/// takes the bytes of the response body.
pub fn client_method(
    method: MethodHandler<'_>,
    form_body: bool,
    path_fields_in_body: bool,
    connect: Option<&ConnectSide>,
) -> Result<TokenStream> {
    let doc_attrs = doc_tokens(method.plan.metadata.documentation.as_deref());
    let method_name = method.plan.base_method_ident();
    let input_type_ident = method.input_type();
    let http_method = format_ident!("{}", method.plan.http_method.to_lowercase());
    let has_query_params = method.plan.url_query_parameters().next().is_some();
    let url_formatting = generate_url_formatting(&method, has_query_params);
    let query_handling = generate_query_parameters(&method);
    let error_module = method.config.client_error_module();
    let result_type = method.result_type();
    let connect = connect.filter(|_| over_connect(&method));
    // A request with no path value, query parameter or body is not read.
    let request_ident = if connect.is_some()
        || method.plan.formats_path_values()
        || has_query_params
        || method.plan.has_request_body
        || !method.plan.request_headers.is_empty()
    {
        format_ident!("request")
    } else {
        format_ident!("_request")
    };
    // The raw body of a client stream: a stream of bytes next to the request.
    let chunks = method
        .plan
        .metadata
        .client_streaming
        .then_some(method.plan.raw_request_body.as_deref())
        .flatten()
        .map(crate::utils::field_ident);
    let stream_param = chunks
        .as_ref()
        .map(|field| quote! { , #field: super::super::ByteStream });
    let stream_arg = chunks.as_ref().map(|field| quote! { , #field });
    // Over Connect, when the client prefers it: the call of connect-rust's client, with the
    // request cloned (it takes it by value) and the answer taken as the owned message.
    let over_connect_name = format_ident!("{}_over_connect", method_name);
    let connect_branch = connect.map(|side| {
        let cfg = &side.cfg;
        quote! {
            #cfg
            {
                if self.protocol == super::super::Protocol::Connect {
                    return self.#over_connect_name(request #stream_arg).await;
                }
            }
        }
    });
    let connect_fn = connect.map(|side| {
        let cfg = &side.cfg;
        let output_type = method.output_type();
        // The answer of a unary call: the owned message, or nothing.
        let (bind, answer) = match &output_type {
            Some(_) => (
                quote! { let response = },
                quote! { Ok(response.into_owned()) },
            ),
            None => (quote! {}, quote! { Ok(()) }),
        };
        let call = match (&chunks, &output_type) {
            // A client stream: every message carries the request's fields, and the chunks go in
            // its raw body. A chunk that fails ends the stream, and its error is the call's.
            (Some(field), _) => quote! {
                let failed = ::std::sync::Arc::new(::std::sync::Mutex::new(None));
                let head = ::futures::stream::iter([#input_type_ident {
                    #field: ::core::default::Default::default(),
                    ..request.clone()
                }]);
                let messages = ::futures::StreamExt::map(#field, {
                    let request = request.clone();
                    let failed = failed.clone();
                    move |chunk| match chunk {
                        Ok(chunk) => Some(#input_type_ident {
                            #field: chunk.to_vec(),
                            ..request.clone()
                        }),
                        Err(e) => {
                            *failed.lock().expect("no panic holds the lock") = Some(e);
                            None
                        }
                    }
                });
                let messages = ::futures::StreamExt::take_while(messages, |message| {
                    ::std::future::ready(message.is_some())
                });
                let messages = ::futures::StreamExt::filter_map(messages, ::std::future::ready);
                let response = self
                    .connect()
                    .#method_name(::futures::StreamExt::chain(head, messages))
                    .await;
                if let Some(e) = failed.lock().expect("no panic holds the lock").take() {
                    return Err(e.into());
                }
                #bind response.map_err(#error_module::from_connect_error)?;
                #answer
            },
            // A server stream of raw bytes: the messages as they come.
            (None, Some(output))
                if method.plan.metadata.server_streaming
                    && method.plan.raw_response_body.is_some() =>
            {
                quote! {
                    let stream = self
                        .connect()
                        .#method_name(request.clone())
                        .await
                        .map_err(#error_module::from_connect_error)?;
                    Ok(Box::pin(::futures::stream::unfold(
                        (stream, false),
                        |(mut stream, failed)| async move {
                            if failed {
                                return None;
                            }
                            match stream.message::<#output>().await {
                                Ok(Some(message)) => {
                                    Some((Ok(message.to_owned_message()), (stream, false)))
                                }
                                Ok(None) => None,
                                Err(e) => Some((
                                    Err(#error_module::from_connect_error(e)),
                                    (stream, true),
                                )),
                            }
                        },
                    )))
                }
            }
            // A server stream of messages: all of them, as the NDJSON lines over REST.
            (None, Some(output)) if method.plan.metadata.server_streaming => quote! {
                let mut stream = self
                    .connect()
                    .#method_name(request.clone())
                    .await
                    .map_err(#error_module::from_connect_error)?;
                let mut outputs = Vec::new();
                while let Some(message) = stream
                    .message::<#output>()
                    .await
                    .map_err(#error_module::from_connect_error)?
                {
                    outputs.push(message.to_owned_message());
                }
                Ok(outputs)
            },
            (None, _) => quote! {
                #bind self
                    .connect()
                    .#method_name(request.clone())
                    .await
                    .map_err(#error_module::from_connect_error)?;
                #answer
            },
        };
        quote! {
            #cfg
            async fn #over_connect_name(
                &self,
                request: &#input_type_ident #stream_param
            ) -> Result<#result_type> {
                #call
            }
        }
    });

    // The path values are in the URL, so the body leaves them out: it is a copy of the request
    // with those fields at their defaults, which the JSON and form serializers skip.
    let path_fields: Vec<_> = if method.plan.has_request_body
        && method.plan.formats_path_values()
        && !path_fields_in_body
    {
        method
            .plan
            .path_parameters()
            .map(|param| crate::utils::field_ident(&param.name))
            .collect()
    } else {
        Vec::new()
    };
    let raw_body = method
        .plan
        .raw_request_body
        .as_deref()
        .map(crate::utils::field_ident);
    // A body that the rule names (`body: "field"`) is that field alone.
    let named_body = method.plan.named_body().map(crate::utils::field_ident);
    let (body_copy, body) = if let Some(field) = &named_body {
        (quote! {}, quote! { &request.#field })
    } else if path_fields.is_empty() || raw_body.is_some() {
        (quote! {}, quote! { request })
    } else {
        let copy = quote! {
            let mut body = request.clone();
            #(body.#path_fields = Default::default();)*
        };
        (copy, quote! { &body })
    };
    // The stream of a client stream as the body: as it comes, or whole in the browser, whose
    // requests take no stream.
    let stream_body = chunks.as_ref().map(|field| {
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            let body = reqwest::Body::wrap_stream(#field);
            #[cfg(target_arch = "wasm32")]
            let body = {
                let mut whole = Vec::new();
                let mut chunks = #field;
                while let Some(chunk) = ::futures::StreamExt::next(&mut chunks).await {
                    whole.extend_from_slice(&chunk?);
                }
                whole
            };
        }
    });
    let body_handling = match (raw_body, method.plan.has_request_body, form_body) {
        (Some(_), _, _) if chunks.is_some() => quote! {
            .header("content-type", "application/octet-stream")
            .body(body)
        },
        (Some(field), _, _) => quote! {
            .header("content-type", "application/octet-stream")
            .body(request.#field.clone())
        },
        (None, true, false) => quote! { .json(#body) },
        (None, true, true) => quote! { .form(#body) },
        (None, false, _) => quote! {},
    };

    // The fields in request headers, when they are set.
    let send = if method.plan.request_headers.is_empty() {
        quote! { let response = self.client.#http_method(url)#body_handling.send().await?; }
    } else {
        let headers = method.plan.request_headers.iter().map(|(field, header)| {
            let field_ident = crate::utils::field_ident(field);
            match method.plan.request_header_is_optional(field) {
                true => quote! {
                    if let Some(ref value) = request.#field_ident {
                        builder = builder.header(#header, value.as_str());
                    }
                },
                false => {
                    quote! { builder = builder.header(#header, request.#field_ident.as_str()); }
                }
            }
        });
        quote! {
            let mut builder = self.client.#http_method(url)#body_handling;
            #(#headers)*
            let response = builder.send().await?;
        }
    };
    let read_response = match method.output_type() {
        Some(output_type) => read_output(&method, &output_type)?,
        None if method.is_existence_check() => quote! { Ok(true) },
        None => quote! { Ok(()) },
    };
    // An existence check answers `false` for a 404 instead of failing.
    let not_found = method.is_existence_check().then(|| {
        quote! {
            if response.status().as_u16() == 404 {
                return Ok(false);
            }
        }
    });

    Ok(quote! {
        #doc_attrs
        pub async fn #method_name(
            &self,
            #request_ident: &#input_type_ident #stream_param
        ) -> Result<#result_type> {
            #connect_branch
            #url_formatting
            #query_handling
            #body_copy
            #stream_body
            #send
            #not_found
            if !response.status().is_success() {
                return Err(#error_module::parse_error_response(response).await);
            }
            #read_response
        }

        #connect_fn
    })
}

/// The statements that turn a successful `response` into the method's output.
fn read_output(
    method: &MethodHandler<'_>,
    output_type: &proc_macro2::Ident,
) -> Result<TokenStream> {
    let lenient = method.config.rest_client.ignore_unknown_enum_values
        && method.config.runtime == Runtime::Buffa;
    let options = lenient.then(|| {
        quote! { let options = ::buffa::json::JsonParseOptions::new().ignore_unknown_enum_values(true); }
    });
    let from_slice = |bytes: TokenStream| {
        if lenient {
            quote! { ::buffa::json::with_json_parse_options(&options, || serde_json::from_slice(#bytes)) }
        } else {
            quote! { serde_json::from_slice(#bytes) }
        }
    };

    let header_fields = method.response_header_fields();
    let mut assignments = header_fields
        .iter()
        .map(|field| header_assignment(method, field))
        .collect::<Result<Vec<_>>>()?;
    // The status field takes the HTTP status of the answer.
    let status = method.status_field().map(|field| {
        let ident = crate::utils::field_ident(&field.name);
        match field.unified_type.is_optional {
            true => quote! { output.#ident = Some(status); },
            false => quote! { output.#ident = status; },
        }
    });
    let read_status = status
        .is_some()
        .then(|| quote! { let status = i32::from(response.status().as_u16()); });
    assignments.extend(status);
    if method.plan.metadata.server_streaming {
        if let Some(field) = &method.plan.raw_response_body {
            // The chunks of the body as the messages, each with the fields that come from the
            // headers and the status of the answer.
            let field_ident = crate::utils::field_ident(field);
            return Ok(quote! {
                let headers = response.headers().clone();
                #read_status
                let chunks = ::futures::StreamExt::map(
                    response.bytes_stream(),
                    move |chunk| -> Result<#output_type> {
                        let mut output = #output_type {
                            #field_ident: chunk?.to_vec(),
                            ..::core::default::Default::default()
                        };
                        #(#assignments)*
                        Ok(output)
                    },
                );
                Ok(Box::pin(chunks))
            });
        }
        // The lines of the NDJSON body; the output fields in response headers are set on every
        // message, as the headers are those of the whole answer.
        let parse_line = from_slice(quote! { line });
        if assignments.is_empty() {
            return Ok(quote! {
                let result = response.bytes().await?;
                #options
                result
                    .split(|byte| *byte == b'\n')
                    .filter(|line| !line.trim_ascii().is_empty())
                    .map(|line| -> Result<#output_type> { Ok(#parse_line?) })
                    .collect()
            });
        }
        return Ok(quote! {
            let headers = response.headers().clone();
            #read_status
            let result = response.bytes().await?;
            #options
            let mut outputs = result
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.trim_ascii().is_empty())
                .map(|line| -> Result<#output_type> { Ok(#parse_line?) })
                .collect::<Result<Vec<#output_type>>>()?;
            for output in &mut outputs {
                #(#assignments)*
            }
            Ok(outputs)
        });
    }

    if let Some(field) = &method.plan.raw_response_body {
        let field_ident = crate::utils::field_ident(field);
        if assignments.is_empty() {
            return Ok(quote! {
                Ok(#output_type {
                    #field_ident: response.bytes().await?.into(),
                    ..::core::default::Default::default()
                })
            });
        }
        return Ok(quote! {
            let headers = response.headers().clone();
            #read_status
            let mut output = #output_type {
                #field_ident: response.bytes().await?.into(),
                ..::core::default::Default::default()
            };
            #(#assignments)*
            Ok(output)
        });
    }

    let parse = from_slice(quote! { &result });
    if assignments.is_empty() {
        return Ok(quote! {
            let result = response.bytes().await?;
            #options
            Ok(#parse?)
        });
    }
    Ok(quote! {
        let headers = response.headers().clone();
        #read_status
        let result = response.bytes().await?;
        #options
        let mut output: #output_type = if result.trim_ascii().is_empty() {
            Default::default()
        } else {
            #parse?
        };
        #(#assignments)*
        Ok(output)
    })
}

/// The statement that sets an output field from its response header, when the header is there and
/// parses.
fn header_assignment(method: &MethodHandler<'_>, field: &MessageField) -> Result<TokenStream> {
    let field_ident = crate::utils::field_ident(&field.name);
    let header = field.response_header.as_deref().unwrap_or_default();
    let ty = &field.unified_type;
    let is_string = match ty.base_type {
        BaseType::String => true,
        BaseType::Int32
        | BaseType::Int64
        | BaseType::Bool
        | BaseType::Float32
        | BaseType::Float64 => false,
        _ => {
            return Err(crate::Error::Build(format!(
                "field `{}` of the output of `{}` comes from a response header, so it must be a string, number or bool",
                field.name, method.plan.handler_function_name
            )));
        }
    };
    if ty.is_repeated {
        return Err(crate::Error::Build(format!(
            "field `{}` of the output of `{}` comes from a response header, so it cannot be repeated",
            field.name, method.plan.handler_function_name
        )));
    }
    let value = quote! { headers.get(#header).and_then(|value| value.to_str().ok()) };
    Ok(match (is_string, ty.is_optional) {
        (true, true) => quote! { output.#field_ident = #value.map(str::to_string); },
        (true, false) => quote! {
            if let Some(value) = #value {
                output.#field_ident = value.to_string();
            }
        },
        (false, true) => {
            quote! { output.#field_ident = #value.and_then(|value| value.parse().ok()); }
        }
        (false, false) => quote! {
            if let Some(value) = #value.and_then(|value| value.parse().ok()) {
                output.#field_ident = value;
            }
        },
    })
}

/// Generate URL formatting code that properly substitutes path parameters.
///
/// `needs_mut` controls whether the generated `url` binding is declared `mut`.
/// Pass `true` only when query parameters will be appended via `url.query_pairs_mut()`.
fn generate_url_formatting(
    method: &MethodHandler<'_>,
    needs_mut: bool,
) -> proc_macro2::TokenStream {
    let path = method.plan.http_pattern.base_path();
    let path = path.trim_start_matches('/');

    // Only the binding's mutability differs between the two callers.
    let mut_kw = if needs_mut {
        quote! { mut }
    } else {
        quote! {}
    };

    // No path values -> direct join; otherwise format then join.
    if !method.plan.formats_path_values() {
        quote! {
            let #mut_kw url = self.base_url.join(#path)?;
        }
    } else {
        let (format_string, format_args) = method.plan.http_pattern.to_format_string();
        let values: Vec<TokenStream> = format_args
            .iter()
            .map(|template_param| {
                let field_ident = crate::utils::field_ident(template_param);
                let keep_slash = method.plan.http_pattern.is_multi_segment(template_param);
                quote! { encode_path_value(&request.#field_ident, #keep_slash) }
            })
            .collect();
        quote! {
            let formatted_path = format!(#format_string, #(#values),*);
            let #mut_kw url = self.base_url.join(&formatted_path)?;
        }
    }
}

/// The helper of the client `mod.rs` that percent-encodes path values, per the gRPC transcoding
/// rules: every byte except the unreserved `A-Z a-z 0-9 - . _ ~` is encoded, and `/` is kept only
/// for values bound to a multi-segment pattern.
pub(crate) fn encode_path_value_fn() -> TokenStream {
    quote! {
        /// Percent-encode a value for a URL path. `keep_slash` keeps `/`, for a value that
        /// spans several path segments; such a value follows a `/` of the template, so its own
        /// leading `/` is dropped.
        fn encode_path_value(value: &impl ::std::fmt::Display, keep_slash: bool) -> String {
            use ::std::fmt::Write as _;
            let value = value.to_string();
            let value = match keep_slash {
                true => value.trim_start_matches('/'),
                false => value.as_str(),
            };
            let mut out = String::with_capacity(value.len());
            for byte in value.bytes() {
                match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                        out.push(byte as char)
                    }
                    b'/' if keep_slash => out.push('/'),
                    _ => {
                        let _ = write!(out, "%{byte:02X}");
                    }
                }
            }
            out
        }
    }
}

/// Generate query parameter handling code
fn generate_query_parameters(method: &MethodHandler<'_>) -> proc_macro2::TokenStream {
    let mut param_assignments = Vec::new();
    for param in method.plan.url_query_parameters() {
        let field_ident = crate::utils::field_ident(&param.name);
        let param_name = method.query_key(param);
        let field_type = &param.field_type;

        if field_type.is_repeated {
            if let BaseType::Enum(enum_name) = &field_type.base_type {
                let enum_type_ident = extract_type_ident(enum_name);
                param_assignments.push(match method.config.runtime {
                    // prost: the field is `Vec<i32>`; recover the typed variant via `from_i32`.
                    Runtime::Prost => quote! {
                        for &raw in &request.#field_ident {
                            if let Some(v) = #enum_type_ident::from_i32(raw) {
                                url.query_pairs_mut().append_pair(#param_name, v.as_str_name());
                            }
                        }
                    },
                    // buffa: the field is `Vec<EnumValue<E>>`; emit only known variants by name.
                    Runtime::Buffa => quote! {
                        for v in &request.#field_ident {
                            if let Some(known) = v.as_known() {
                                use buffa::Enumeration as _;
                                url.query_pairs_mut().append_pair(#param_name, known.proto_name());
                            }
                        }
                    },
                });
            } else {
                param_assignments.push(quote! {
                    for value in &request.#field_ident {
                        url.query_pairs_mut().append_pair(#param_name, &value.to_string());
                    }
                });
            }
        } else if matches!(method.config.runtime, Runtime::Buffa)
            && matches!(field_type.base_type, BaseType::Enum(_))
        {
            // buffa singular enum: the field is `EnumValue<E>` (or `Option<EnumValue<E>>`), which
            // has no `Display`. Emit the known variant by its proto name; skip unknown/absent.
            if param.is_optional() {
                param_assignments.push(quote! {
                    if let Some(known) = request.#field_ident.and_then(|v| v.as_known()) {
                        use buffa::Enumeration as _;
                        url.query_pairs_mut().append_pair(#param_name, known.proto_name());
                    }
                });
            } else {
                param_assignments.push(quote! {
                    if let Some(known) = request.#field_ident.as_known() {
                        use buffa::Enumeration as _;
                        url.query_pairs_mut().append_pair(#param_name, known.proto_name());
                    }
                });
            }
        } else if param.is_optional() {
            param_assignments.push(quote! {
                if let Some(ref value) = request.#field_ident {
                    url.query_pairs_mut().append_pair(#param_name, &value.to_string());
                }
            });
        } else if matches!(field_type.base_type, BaseType::String) {
            // Required string param: always sent. Append the value directly (it is already a
            // `String`, so `.to_string()` would be redundant).
            param_assignments.push(quote! {
                url.query_pairs_mut().append_pair(#param_name, &request.#field_ident);
            });
        } else {
            // Required non-string scalar: always sent.
            param_assignments.push(quote! {
                url.query_pairs_mut().append_pair(#param_name, &request.#field_ident.to_string());
            });
        }
    }

    if param_assignments.is_empty() {
        return quote! {};
    }

    quote! {
        #(#param_assignments)*
    }
}
