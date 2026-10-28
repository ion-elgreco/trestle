//! Axum server-side glue generation.
//!
//! This is part of the **Generation** stage of the Analysis → Planning → Generation → Output
//! pipeline (see [`super`]). It emits the wiring that connects incoming HTTP requests to the
//! handler trait produced by [`super::handler`]:
//!
//! - `axum::extract::FromRequestParts` extractors for path and query parameters and
//!   `axum::extract::FromRequest` extractors for the JSON body (the "common" output, always
//!   generated);
//! - per-method route handler functions that build the typed request, call the handler trait, and
//!   serialize the response (the "server" output).
//!
//! Which extractors and imports are emitted is driven by the per-method
//! [`GenerationPlan`](crate::analysis::GenerationPlan) computed during Planning (e.g. only services
//! with path/query params pull in `RequestPartsExt`). Emitted token streams are pretty-printed via
//! `super::format_tokens`.

use itertools::Itertools;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{Path, Type};

use super::format_tokens;
use super::validate::has_required_fields;
use crate::{
    Error,
    analysis::{MethodPlan, RequestParam},
    codegen::{CodeGenConfig, MethodHandler, Runtime, ServiceHandler},
    parsing::MessageField,
    parsing::http::HttpPattern,
    parsing::types::{BaseType, RenderContext},
};

/// The extractor types and the bad-request answer that the generated code uses: the app's
/// ([`ServerOptions::extractors`](super::ServerOptions::extractors)) or axum's.
struct Extractors {
    json: Path,
    query: Path,
    path: Path,
    /// The answer to a request that fails `validate()`, with the error as `e`.
    invalid_argument: TokenStream,
}

impl Extractors {
    fn of(config: &CodeGenConfig) -> Self {
        match &config.server.extractors {
            Some(module) => {
                let module: Path = syn::parse_str(module).expect("valid extractors path");
                Self {
                    json: syn::parse_quote! { #module::Json },
                    query: syn::parse_quote! { #module::Query },
                    path: syn::parse_quote! { #module::Path },
                    invalid_argument: quote! { #module::invalid_argument(e.to_string()) },
                }
            }
            None => Self {
                json: syn::parse_quote! { axum::Json },
                query: syn::parse_quote! { axum_extra::extract::Query },
                path: syn::parse_quote! { axum::extract::Path },
                invalid_argument: quote! {
                    axum::response::IntoResponse::into_response((
                        axum::http::StatusCode::BAD_REQUEST,
                        e.to_string(),
                    ))
                },
            },
        }
    }
}

/// Generate server side code for axum servers
///
/// This generates:
/// - FromRequestParts extractor implementations for path/query parameters
/// - FromRequest extractor implementations for JSON body
///
/// A method with a raw body gets no extractor: the server emitter leaves it to the app.
pub(super) fn generate_common(service: &ServiceHandler<'_>) -> crate::error::Result<String> {
    let extractors = Extractors::of(service.config);
    let methods = server_methods(service);
    // One extractor per request message: the methods that share a message read it the same
    // way (`server_methods`).
    let mut extracted = std::collections::HashSet::new();
    let extractor_impls = methods
        .iter()
        .filter(|method| extracted.insert(method.plan.metadata.input_type.clone()))
        .map(|method| from_request_extractor(method, &extractors))
        .collect_vec();
    let mod_path = service.models_path_crate();

    // Only import RequestPartsExt when there are FromRequestParts impls (path/query params).
    let has_parts_extractors = methods.iter().any(|m| extracts_from_parts(m));

    let axum_imports = if has_parts_extractors {
        quote! { use axum::{RequestExt, RequestPartsExt}; }
    } else {
        quote! { use axum::RequestExt; }
    };

    // NB: these extractor impls return `Result<Self, Self::Rejection>` — the
    // 2-arg std `Result` from the prelude, NOT the app's 1-arg `result_type`
    // alias. This file lands in the models crate (`output_common`), which has no
    // `crate::api`, so importing the configured `result_type` here would be both
    // unresolvable and an arity mismatch. Do not import it.
    let tokens = quote! {
        // `unused_imports`: `RequestExt`/the model glob are only used by some
        // extractor shapes. `unused_mut`: the `mut req`/`mut parts` bindings
        // aren't always mutated.
        #![allow(unused_mut, unused_imports)]
        use #mod_path::*;
        #axum_imports

        #(#extractor_impls)*
    };

    format_tokens(tokens)
}

pub(super) fn generate_server(service: &ServiceHandler<'_>) -> crate::error::Result<String> {
    let extractors = Extractors::of(service.config);
    let empty_response: Option<Path> = service
        .config
        .server
        .empty_response
        .as_deref()
        .map(|path| syn::parse_str(path).expect("valid empty_response path"));
    let methods = server_methods(service);
    let handler_function_impls = methods
        .iter()
        .map(|method| {
            axum_route_handler_impl(
                method,
                &service.plan.handler_name,
                &extractors,
                empty_response.as_ref(),
            )
        })
        .collect::<crate::error::Result<Vec<_>>>()?;
    let router = router_fn(service, &methods)?;

    let mod_path = service.models_path();
    let foreign_models = service.foreign_models_imports();
    // handler_name is a validated Rust identifier, so this parse is infallible.
    let trait_path: Path =
        syn::parse_str(&format!("super::handler::{}", service.plan.handler_name)).unwrap();
    let result_path: Path =
        syn::parse_str(&service.config.result_type_path).expect("valid result_type_path");

    let tokens = quote! {
        // Generated route fns: `too_many_arguments` for flat methods with many
        // path/query params; `unused_mut` for the `State`/extractor bindings.
        #![allow(unused_mut, clippy::too_many_arguments)]
        use #result_path;
        use #mod_path::*;
        #(#foreign_models)*
        use #trait_path;
        use axum::extract::State;

        #(#handler_function_impls)*

        #router
    };

    format_tokens(tokens)
}

/// The `router()` of the service: every route with its handler, for an app to merge.
fn router_fn(
    service: &ServiceHandler<'_>,
    methods: &[MethodHandler<'_>],
) -> crate::error::Result<TokenStream> {
    let handler_trait = format_ident!("{}", service.plan.handler_name);
    let mut routes: Vec<(String, Vec<TokenStream>)> = Vec::new();
    for method in methods {
        let path = axum_path(&method.plan.http_pattern);
        let verb = match method.plan.http_method.to_ascii_uppercase().as_str() {
            "GET" => "get",
            "POST" => "post",
            "PUT" => "put",
            "PATCH" => "patch",
            "DELETE" => "delete",
            "HEAD" => "head",
            other => {
                return Err(Error::Build(format!(
                    "no axum route for the HTTP method `{other}` of `{}`",
                    method.plan.handler_function_name
                )));
            }
        };
        let verb = format_ident!("{verb}");
        let handler = format_ident!("{}", method.plan.handler_function_name);
        let call = quote! { #verb(#handler::<T, Cx>) };
        match routes.iter_mut().find(|(p, _)| *p == path) {
            Some((_, calls)) => calls.push(call),
            None => routes.push((path, vec![call])),
        }
    }
    let routes = routes.into_iter().map(|(path, calls)| {
        let (first, rest) = calls.split_first().expect("a route has a method");
        quote! { .route(#path, axum::routing::#first #(.#rest)*) }
    });
    Ok(quote! {
        /// The routes of the service with their handlers, for an app to merge: the app's
        /// state (`T`) implements the handler trait, and `Cx` is the context of its requests.
        pub fn router<T, Cx>() -> axum::Router<T>
        where
            T: #handler_trait<Cx> + Clone + Send + Sync + 'static,
            Cx: axum::extract::FromRequestParts<T> + Send + 'static,
        {
            axum::Router::new() #(#routes)*
        }
    })
}

/// The axum form of a route template: `{name}` for a path parameter, `{*name}` for one of
/// several segments (`{name=**}`), the rest as in the template.
fn axum_path(pattern: &HttpPattern) -> String {
    let mut path = String::new();
    let mut rest = pattern.template.as_str();
    while let Some(start) = rest.find('{') {
        path.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .map_or(rest.len(), |end| start + end + 1);
        let parameter = &rest[start + 1..end - 1];
        let name = parameter.split('=').next().unwrap_or(parameter);
        if pattern.multi_segment_parameters.iter().any(|p| p == name) {
            path.push_str(&format!("{{*{name}}}"));
        } else {
            path.push_str(&format!("{{{name}}}"));
        }
        rest = &rest[end..];
    }
    path.push_str(rest);
    path
}

/// The `validate()` call of a request whose message has `REQUIRED` fields, answered with the
/// extractors' bad request.
fn validation(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    if !has_required_fields(method.metadata, &method.plan.metadata.input_type) {
        return quote! {};
    }
    let invalid_argument = &extractors.invalid_argument;
    quote! {
        request.validate().map_err(|e| #invalid_argument)?;
    }
}

/// The routed methods that the server emitter supports: a route and a handler method each, and
/// an extractor per request message. Methods that share a request message share its extractor
/// when they read the message the same way (the same path parameters, query, headers and
/// body). Left out, for the app to write by hand: a method that sends or reads a raw body
/// (`(olai.trestle.v1.raw_body)`), and a method whose request message another method of the
/// service reads in another way.
pub(super) fn server_methods<'a>(service: &'a ServiceHandler<'a>) -> Vec<MethodHandler<'a>> {
    let extractors = Extractors::of(service.config);
    // The extractor of each request message, by message.
    let mut extracted: Vec<(String, String)> = Vec::new();
    service
        .rest_methods()
        .filter(|method| {
            if method.plan.raw_request_body.is_some() || method.plan.raw_response_body.is_some() {
                return false;
            }
            let input = &method.plan.metadata.input_type;
            let extractor = from_request_extractor(method, &extractors).to_string();
            match extracted.iter().find(|(message, _)| message == input) {
                Some((_, same)) if *same == extractor => true,
                Some(_) => {
                    tracing::warn!(
                        "no server route for `{}`: another method reads `{input}` in another way",
                        method.plan.handler_function_name
                    );
                    false
                }
                None => {
                    extracted.push((input.clone(), extractor));
                    true
                }
            }
        })
        .collect()
}

/// Whether the request is read from the request parts alone (path and query, `FromRequestParts`)
/// rather than with a body (`FromRequest`). A GET with a body reads the body.
fn extracts_from_parts(method: &MethodHandler<'_>) -> bool {
    method.plan.needs_request_parts && method.plan.body_fields().next().is_none()
}

/// Generate extractor implementation for a specific method.
///
/// Path/query-only methods use `FromRequestParts`; body-bearing methods use `FromRequest`. The
/// split is precomputed on the plan (`needs_request_parts`) so this no longer re-matches the
/// `RequestType` enum.
fn from_request_extractor(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    if extracts_from_parts(method) {
        from_request_parts_impl(method, extractors)
    } else {
        from_request_impl(method, extractors)
    }
}

/// Generate route handler function: the request through the handler trait, its answer as JSON
/// with the status of `(olai.trestle.v1.response_status)`, and the output fields that go in
/// response headers (`(olai.trestle.v1.response_header)`) set on the answer.
fn axum_route_handler_impl(
    method: &MethodHandler<'_>,
    handler_trait: &str,
    extractors: &Extractors,
    empty_response: Option<&Path>,
) -> crate::error::Result<TokenStream> {
    let handler_method = format_ident!("{}", method.plan.handler_function_name);
    let input_type = method.input_type();
    let handler_trait_ident = format_ident!("{}", handler_trait);
    let json = &extractors.json;
    let response_status = method.plan.metadata.response_status;
    let status = response_status
        .map(|status| status_code(method, status))
        .transpose()?;

    let (output, body) = if method.plan.has_response {
        if response_status == Some(204) {
            return Err(Error::Build(format!(
                "method `{}` answers 204 (no content), so it cannot have an output",
                method.plan.handler_function_name
            )));
        }
        let output_type = method.output_type();
        let header_sets = method
            .response_header_fields()
            .into_iter()
            .map(|field| header_set(method, field))
            .collect::<crate::error::Result<Vec<_>>>()?;
        if method.plan.metadata.server_streaming {
            // A server stream over REST: an NDJSON body, one line per message. The status, and
            // the headers that come from the first message, go out before the body; an error
            // before the first message is the answer, one after it ends the body.
            let headers = (!header_sets.is_empty()).then(|| {
                quote! {
                    if let Some(result) = &first {
                        #(#header_sets)*
                    }
                }
            });
            let status = status.map(|status| quote! { #status, });
            (
                quote! { axum::response::Response },
                quote! {
                    let mut stream = handler.#handler_method(request, context).await?;
                    let first = ::futures::StreamExt::next(&mut stream).await.transpose()?;
                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("application/x-ndjson"),
                    );
                    #headers
                    let lines = ::futures::StreamExt::map(
                        ::futures::StreamExt::chain(::futures::stream::iter(first.map(Ok)), stream),
                        |message| -> ::core::result::Result<axum::body::Bytes, axum::BoxError> {
                            match message {
                                Ok(message) => {
                                    let mut line = serde_json::to_vec(&message)?;
                                    line.push(b'\n');
                                    Ok(line.into())
                                }
                                Err(_) => Err("the stream failed".into()),
                            }
                        },
                    );
                    Ok(axum::response::IntoResponse::into_response((
                        #status
                        headers,
                        axum::body::Body::from_stream(lines),
                    )))
                },
            )
        } else {
            // The answer: the status when it is not 200, the headers when there are some, and
            // the JSON body.
            let mut types = Vec::new();
            let mut values = Vec::new();
            if let Some(status) = &status {
                types.push(quote! { axum::http::StatusCode });
                values.push(status.clone());
            }
            let headers = if header_sets.is_empty() {
                quote! {}
            } else {
                types.push(quote! { axum::http::HeaderMap });
                values.push(quote! { headers });
                quote! {
                    let mut headers = axum::http::HeaderMap::new();
                    #(#header_sets)*
                }
            };
            types.push(quote! { #json<#output_type> });
            values.push(quote! { #json(result) });
            let (output, answer) = if types.len() == 1 {
                (types.remove(0), values.remove(0))
            } else {
                (quote! { (#(#types),*) }, quote! { (#(#values),*) })
            };
            (
                output,
                quote! {
                    let result = handler.#handler_method(request, context).await?;
                    #headers
                    Ok(#answer)
                },
            )
        }
    } else {
        let (output, answer) = match (&status, empty_response) {
            // A 204 answer has no body, not even the empty one.
            (Some(status), Some(empty)) if response_status != Some(204) => (
                quote! { (axum::http::StatusCode, #empty) },
                quote! { (#status, ::core::default::Default::default()) },
            ),
            (Some(status), _) => (quote! { axum::http::StatusCode }, quote! { #status }),
            (None, Some(empty)) => (
                quote! { #empty },
                quote! { ::core::default::Default::default() },
            ),
            (None, None) => (quote! { () }, quote! { () }),
        };
        (
            output,
            quote! {
                handler.#handler_method(request, context).await?;
                Ok(#answer)
            },
        )
    };
    Ok(quote! {
        pub async fn #handler_method<T, Cx>(
            State(handler): State<T>,
            context: Cx,
            request: #input_type,
        ) -> Result<#output>
        where
            T: #handler_trait_ident<Cx> + Clone + Send + Sync + 'static,
            Cx: axum::extract::FromRequestParts<T> + Send,
        {
            #body
        }
    })
}

/// The `StatusCode` of a `(olai.trestle.v1.response_status)`.
fn status_code(method: &MethodHandler<'_>, status: u16) -> crate::error::Result<TokenStream> {
    let name = match status {
        200 => Some("OK"),
        201 => Some("CREATED"),
        202 => Some("ACCEPTED"),
        204 => Some("NO_CONTENT"),
        100..=599 => None,
        _ => {
            return Err(Error::Build(format!(
                "method `{}`: `{status}` is not an HTTP status",
                method.plan.handler_function_name
            )));
        }
    };
    Ok(match name {
        Some(name) => {
            let name = format_ident!("{name}");
            quote! { axum::http::StatusCode::#name }
        }
        None => quote! { axum::http::StatusCode::from_u16(#status).expect("an HTTP status") },
    })
}

/// The statement that puts the output field `field` in its response header: a string when it
/// is there and not empty, a number or a bool as text.
fn header_set(
    method: &MethodHandler<'_>,
    field: &MessageField,
) -> crate::error::Result<TokenStream> {
    let ident = crate::utils::field_ident(&field.name);
    let header = field
        .response_header
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let ty = &field.unified_type;
    let is_string = match ty.base_type {
        BaseType::String => true,
        BaseType::Int32
        | BaseType::Int64
        | BaseType::Bool
        | BaseType::Float32
        | BaseType::Float64 => false,
        _ => {
            return Err(Error::Build(format!(
                "field `{}` of the output of `{}` goes in a response header, so it must be a string, number or bool",
                field.name, method.plan.handler_function_name
            )));
        }
    };
    if ty.is_repeated {
        return Err(Error::Build(format!(
            "field `{}` of the output of `{}` goes in a response header, so it cannot be repeated",
            field.name, method.plan.handler_function_name
        )));
    }
    let insert = quote! { headers.insert(#header, value); };
    // The header value, when the field has one: one `if let`, as clippy asks.
    Ok(match (ty.is_optional, is_string) {
        (true, true) => quote! {
            if let Some(value) = result.#ident
                .as_deref()
                .and_then(|value| axum::http::HeaderValue::from_str(value).ok())
            {
                #insert
            }
        },
        (false, true) => quote! {
            if let Some(value) = Some(result.#ident.as_str())
                .filter(|value| !value.is_empty())
                .and_then(|value| axum::http::HeaderValue::from_str(value).ok())
            {
                #insert
            }
        },
        (true, false) => quote! {
            if let Some(value) = result.#ident
                .and_then(|value| axum::http::HeaderValue::from_str(&value.to_string()).ok())
            {
                #insert
            }
        },
        (false, false) => quote! {
            if let Ok(value) = axum::http::HeaderValue::from_str(&result.#ident.to_string()) {
                #insert
            }
        },
    })
}

/// The request fields in request headers (`(olai.trestle.v1.request_header)`), read from the
/// parts: a missing header is `None`, or empty for a field that is not `optional` (a `REQUIRED`
/// one fails `validate()`).
fn header_extractions(method: &MethodHandler<'_>) -> TokenStream {
    let reads = method.plan.request_headers.iter().map(|(field, header)| {
        let ident = crate::utils::field_ident(field);
        let value = quote! {
            parts
                .headers
                .get(#header)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        if method.plan.request_header_is_optional(field) {
            quote! { let #ident = #value; }
        } else {
            quote! { let #ident = #value.unwrap_or_default(); }
        }
    });
    quote! { #(#reads)* }
}

/// Generate FromRequestParts implementation for path/query parameters
fn from_request_parts_impl(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    let input_type = method.input_type();
    let path_extractions = path_extractions(method, extractors);
    let query_extractions = query_extractions(method, extractors);
    let header_extractions = header_extractions(method);
    let field_assignments = field_assignments(method.plan, method.config.runtime);
    let validation = validation(method, extractors);
    // A request without path or query parameters reads nothing from the parts.
    let parts = if method.plan.parameters.is_empty() {
        quote! { _parts }
    } else {
        quote! { parts }
    };

    quote! {
        impl<S: Send + Sync> axum::extract::FromRequestParts<S> for #input_type {
            type Rejection = axum::response::Response;

            async fn from_request_parts(
                #parts: &mut axum::http::request::Parts,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                #path_extractions
                #query_extractions
                #header_extractions

                let request = #input_type {
                    #field_assignments
                };
                #validation
                Ok(request)
            }
        }
    }
}

/// Generate FromRequest implementation for JSON body
fn from_request_impl(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    let input_type = method.input_type();
    let json = &extractors.json;
    let validation = validation(method, extractors);

    let is_hybrid = method
        .plan
        .parameters
        .iter()
        .any(|param| matches!(param, RequestParam::Path(_) | RequestParam::Query(_)));

    // Check if we need a hybrid extractor (path/query + body)
    if is_hybrid {
        // Generate hybrid implementation
        generate_hybrid_request_impl(method, extractors)
    } else {
        // Simple JSON body extraction
        quote! {
            impl<S: Send + Sync> axum::extract::FromRequest<S> for #input_type {
                type Rejection = axum::response::Response;

                async fn from_request(
                    req: axum::extract::Request<axum::body::Body>,
                    _state: &S,
                ) -> Result<Self, Self::Rejection> {
                    let #json(request) = req
                        .extract::<#json<#input_type>, _>()
                        .await
                        .map_err(axum::response::IntoResponse::into_response)?;
                    #validation
                    Ok(request)
                }
            }
        }
    }
}

/// Generate hybrid FromRequest implementation for methods with path/query + body
fn generate_hybrid_request_impl(
    method: &MethodHandler<'_>,
    extractors: &Extractors,
) -> TokenStream {
    // Only reached when is_hybrid == true (caller checked path/query params exist),
    // which requires a non-Empty input message, so input_type() is always Some here.
    let input_type = method.input_type().unwrap();
    let path_extractions = path_extractions(method, extractors);
    let query_extractions = query_extractions(method, extractors);
    let header_extractions = header_extractions(method);
    // Oneof fields deserialize from JSON like any other field, so no special treatment needed.
    let body_extractions = generate_body_extractions_tokens(method, &input_type, extractors);
    let field_assignments = field_assignments(method.plan, method.config.runtime);
    let validation = validation(method, extractors);

    // When the method has no body fields, the request body is unused — bind it to
    // `_body` so it doesn't trip `unused_variables`.
    let body_binding = if method.plan.body_fields().next().is_some() {
        quote! { body }
    } else {
        quote! { _body }
    };

    quote! {
        impl<S: Send + Sync> axum::extract::FromRequest<S> for #input_type {
            type Rejection = axum::response::Response;

            async fn from_request(
                mut req: axum::extract::Request<axum::body::Body>,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                // Extract path and query parameters
                let (mut parts, #body_binding) = req.into_parts();
                #path_extractions
                #query_extractions
                #header_extractions

                // Extract body fields (only when the request has a body).
                #body_extractions

                let request = #input_type {
                    #field_assignments
                };
                #validation
                Ok(request)
            }
        }
    }
}

/// Generate body parameter extractions as TokenStream. The body is the request message without
/// its path and query fields, or the field that the HTTP rule names (`body: "field"`).
fn generate_body_extractions_tokens(
    method: &MethodHandler<'_>,
    response_type: &Ident,
    extractors: &Extractors,
) -> TokenStream {
    let body_fields = method.plan.body_fields().collect_vec();
    if body_fields.is_empty() {
        return quote! {};
    }
    let json = &extractors.json;
    let named = method
        .plan
        .named_body()
        .and_then(|name| body_fields.iter().find(|field| field.name == name));
    if let Some(field) = named {
        // The body is the field itself, parsed as its type; the other body fields (oneofs) are
        // not in it.
        let ident = crate::utils::field_ident(&field.name);
        let field_type = method.field_type(
            &crate::parsing::types::UnifiedType {
                is_optional: false,
                ..field.field_type.clone()
            },
            RenderContext::Extractor,
        );
        let others = body_fields
            .iter()
            .filter(|other| other.name != field.name)
            .map(|other| {
                let ident = crate::utils::field_ident(&other.name);
                quote! { let #ident = ::core::default::Default::default(); }
            });
        quote! {
            let body_req = axum::extract::Request::from_parts(parts, body);
            let #json::<#field_type>(body) = body_req
                .extract()
                .await
                .map_err(axum::response::IntoResponse::into_response)?;
            let #ident = body.into();
            #(#others)*
        }
    } else {
        let field_names: Vec<_> = body_fields
            .iter()
            .map(|f| format_ident!("{}", f.name))
            .collect();
        // A single body field is a plain binding, not a 1-tuple destructure — `(x) = (y)`
        // trips `unused_parens`.
        let binding = if field_names.len() == 1 {
            let name = &field_names[0];
            quote! { let #name = body.#name; }
        } else {
            quote! { let (#(#field_names),*) = (#(body.#field_names),*); }
        };
        quote! {
            let body_req = axum::extract::Request::from_parts(parts, body);
            let #json::<#response_type>(body) = body_req
                .extract()
                .await
                .map_err(axum::response::IntoResponse::into_response)?;
            #binding
        }
    }
}

/// Generate path parameter extractions as TokenStream
fn path_extractions(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    let params = &method.plan.path_parameters().collect_vec();

    if params.is_empty() {
        quote! {}
    } else {
        let param_names: Vec<Ident> = params.iter().map(|p| format_ident!("{}", p.name)).collect();
        let param_types: Vec<Type> = params
            .iter()
            .map(|p| method.field_type(&p.field_type, RenderContext::Extractor))
            .collect();

        // A single path param is a scalar, not a 1-tuple: `Path(name)` / `Path<String>`.
        // Wrapping it in parens (`Path((name))`) trips the `unused_parens` lint.
        let (pat, ty) = if param_names.len() == 1 {
            let name = &param_names[0];
            let ty = &param_types[0];
            (quote! { #name }, quote! { #ty })
        } else {
            (
                quote! { (#(#param_names),*) },
                quote! { (#(#param_types),*) },
            )
        };

        let path = &extractors.path;
        quote! {
            let #path(#pat) = parts
                .extract::<#path<#ty>>()
                .await
                .map_err(axum::response::IntoResponse::into_response)?;
        }
    }
}

/// Generate query parameter extractions as TokenStream: the query parameters in the URL, not
/// the ones in request headers.
fn query_extractions(method: &MethodHandler<'_>, extractors: &Extractors) -> TokenStream {
    let params = method.plan.url_query_parameters().collect_vec();
    if params.is_empty() {
        quote! {}
    } else {
        let query_fields = params.iter().map(|p| {
            let name = format_ident!("{}", p.name);
            // Use QueryExtractor so enums render as their actual type (not i32):
            // query strings carry variant names as strings, not integers.
            let type_tokens = method.field_type(&p.field_type, RenderContext::QueryExtractor);
            // Optional and repeated fields get #[serde(default)] so an absent key deserializes to
            // None / an empty Vec rather than erroring. Required query params (proto3 fields not
            // marked `optional`) intentionally have NO default: omitting one is a 400/422, which
            // is the correct contract for a required parameter.
            // The query key that the client sends.
            let key = method.query_key(p);
            let rename = (key != p.name).then(|| quote! { #[serde(rename = #key)] });
            if p.is_optional() || p.field_type.is_repeated {
                quote! { #rename #[serde(default)] #name: #type_tokens }
            } else {
                quote! { #rename #name: #type_tokens }
            }
        });

        let param_names: Vec<Ident> = params.iter().map(|p| format_ident!("{}", p.name)).collect();
        let query = &extractors.query;

        quote! {
            #[derive(serde::Deserialize)]
            struct QueryParams {
                #(#query_fields,)*
            }
            // axum_extra::extract::Query uses serde_html_form which supports repeated query
            // parameters (?foo=a&foo=b → Vec<T>), unlike axum::extract::Query (serde_urlencoded).
            let #query(QueryParams { #(#param_names),* }) = parts
                .extract::<#query<QueryParams>>()
                .await
                .map_err(axum::response::IntoResponse::into_response)?;
        }
    }
}

/// Generate field assignments for request struct construction as TokenStream.
///
/// Enum query params are extracted as their actual Rust enum type (via the `QueryExtractor`
/// context) but the request struct stores enums in the consumed runtime's representation, so
/// we wrap here: `as i32` for prost, `buffa::EnumValue::Known(..)` for buffa.
fn field_assignments(method: &MethodPlan, runtime: Runtime) -> TokenStream {
    let assignments = method.parameters.iter().map(|param| {
        let ident = param.field_ident();
        match param {
            RequestParam::Query(q) if matches!(q.field_type.base_type, BaseType::Enum(_)) => {
                match runtime {
                    Runtime::Prost if q.field_type.is_repeated => {
                        quote! { #ident: #ident.into_iter().map(|v| v as i32).collect() }
                    }
                    Runtime::Prost if q.is_optional() => {
                        quote! { #ident: #ident.map(|v| v as i32) }
                    }
                    Runtime::Prost => quote! { #ident: #ident as i32 },
                    Runtime::Buffa if q.field_type.is_repeated => {
                        quote! { #ident: #ident.into_iter().map(buffa::EnumValue::Known).collect() }
                    }
                    Runtime::Buffa if q.is_optional() => {
                        quote! { #ident: #ident.map(buffa::EnumValue::Known) }
                    }
                    Runtime::Buffa => quote! { #ident: buffa::EnumValue::Known(#ident) },
                }
            }
            _ => quote! { #ident },
        }
    });
    // buffa messages carry a hidden `__buffa_unknown_fields` field, so the
    // struct literal can't be exhaustive — close it with `..Default::default()`.
    // prost requests, by contrast, are fully composed of path/query/body params,
    // so the literal is already exhaustive and a spread would be a
    // `clippy::needless_update` warning. Only emit it for buffa.
    match runtime {
        Runtime::Buffa => quote! { #(#assignments,)* ..Default::default() },
        Runtime::Prost => quote! { #(#assignments,)* },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{QueryParam, RequestParam};
    use crate::parsing::types::{BaseType, UnifiedType};

    fn make_query_plan(params: Vec<RequestParam>) -> MethodPlan {
        use crate::analysis::RequestType;
        use crate::google::api::{HttpRule, http_rule::Pattern};
        use crate::parsing::{HttpPattern, MethodMetadata};
        MethodPlan {
            metadata: MethodMetadata {
                service_name: "TestService".to_string(),
                method_name: "ListThings".to_string(),
                input_type: "ListThingsRequest".to_string(),
                output_type: "ListThingsResponse".to_string(),
                server_streaming: false,
                no_side_effects: false,
                response_status: None,
                operation: None,
                http_rule: HttpRule {
                    selector: "".to_string(),
                    pattern: Some(Pattern::Get("/things".to_string())),
                    body: "".to_string(),
                    response_body: "".to_string(),
                    additional_bindings: vec![],
                },
                http_pattern: HttpPattern::parse("/things"),
                documentation: None,
            },
            handler_function_name: "list_things".to_string(),
            http_pattern: HttpPattern::parse("/things"),
            http_method: "GET".to_string(),
            parameters: params,
            has_response: true,
            request_type: RequestType::List,
            output_resource_type: None,
            // Values for a List method: read-only (no body), extracted from request parts.
            has_request_body: false,
            needs_request_parts: true,
            scoped_verb: None,
            // Server extractor tests don't consult `shape`; the test service has no resource.
            shape: crate::analysis::MethodShape::Unbound,
            has_http_route: true,
            raw_request_body: None,
            raw_response_body: None,
            request_headers: Vec::new(),
        }
    }

    fn repeated_string_param(name: &str) -> RequestParam {
        RequestParam::Query(QueryParam {
            name: name.to_string(),
            json_name: None,
            field_type: UnifiedType {
                base_type: BaseType::String,
                is_optional: false,
                is_repeated: true,
            },
            documentation: None,
            resource_reference: None,
        })
    }

    fn optional_enum_param(name: &str) -> RequestParam {
        RequestParam::Query(QueryParam {
            name: name.to_string(),
            json_name: None,
            field_type: UnifiedType {
                base_type: BaseType::Enum("example.items.v1.ItemType".to_string()),
                is_optional: true,
                is_repeated: false,
            },
            documentation: None,
            resource_reference: None,
        })
    }

    fn repeated_enum_param(name: &str) -> RequestParam {
        RequestParam::Query(QueryParam {
            name: name.to_string(),
            json_name: None,
            field_type: UnifiedType {
                base_type: BaseType::Enum("example.items.v1.ItemType".to_string()),
                is_optional: false,
                is_repeated: true,
            },
            documentation: None,
            resource_reference: None,
        })
    }

    #[test]
    fn test_field_assignments_repeated_string_uses_shorthand() {
        let plan = make_query_plan(vec![repeated_string_param("tags")]);
        let tokens = field_assignments(&plan, Runtime::Prost).to_string();
        // Repeated strings use struct shorthand (no cast needed)
        assert!(tokens.contains("tags"), "should emit 'tags'");
        assert!(!tokens.contains("as i32"), "should not cast string to i32");
    }

    #[test]
    fn test_field_assignments_optional_enum_casts_to_i32() {
        let plan = make_query_plan(vec![optional_enum_param("item_type")]);
        let tokens = field_assignments(&plan, Runtime::Prost).to_string();
        assert!(
            tokens.contains("map"),
            "optional enum should use .map(|v| v as i32)"
        );
        assert!(tokens.contains("as i32"), "should cast enum to i32");
    }

    #[test]
    fn test_field_assignments_repeated_enum_collects_as_i32() {
        let plan = make_query_plan(vec![repeated_enum_param("item_types")]);
        let tokens = field_assignments(&plan, Runtime::Prost).to_string();
        assert!(
            tokens.contains("into_iter"),
            "repeated enum should use into_iter().map(|v| v as i32).collect()"
        );
        assert!(
            tokens.contains("as i32"),
            "should cast enum variants to i32"
        );
    }

    #[test]
    fn test_field_assignments_optional_enum_buffa_wraps_enum_value() {
        let plan = make_query_plan(vec![optional_enum_param("item_type")]);
        let tokens = field_assignments(&plan, Runtime::Buffa).to_string();
        assert!(
            tokens.contains("EnumValue :: Known"),
            "optional enum should wrap with buffa::EnumValue::Known under buffa, got: {tokens}"
        );
        assert!(
            !tokens.contains("as i32"),
            "buffa should not cast enum to i32"
        );
    }

    #[test]
    fn test_field_assignments_repeated_enum_buffa_collects_enum_value() {
        let plan = make_query_plan(vec![repeated_enum_param("item_types")]);
        let tokens = field_assignments(&plan, Runtime::Buffa).to_string();
        assert!(
            tokens.contains("into_iter") && tokens.contains("EnumValue :: Known"),
            "repeated enum should collect via buffa::EnumValue::Known under buffa, got: {tokens}"
        );
        assert!(
            !tokens.contains("as i32"),
            "buffa should not cast enum variants to i32"
        );
    }
}
