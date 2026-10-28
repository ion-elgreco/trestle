//! The `routes` module of the models: the table of the routed RPCs of the API, with the JSON
//! round trip of their request and response messages. With
//! [`RestClientOptions::route_table`](super::RestClientOptions::route_table) the generator
//! appends it to the models `mod.rs`.

use proc_macro2::TokenStream;
use quote::quote;

use super::config::ModelsPath;
use super::{
    CodeGenConfig, ServiceHandler, extract_type_ident, format_tokens_static, message_package,
    package_models_segment, package_models_version,
};
use crate::analysis::GenerationPlan;
use crate::parsing::CodeGenMetadata;

/// The `routes` module.
pub(crate) fn module(
    plan: &GenerationPlan,
    metadata: &CodeGenMetadata,
    config: &CodeGenConfig,
) -> String {
    let models = ModelsPath::from_template(&config.models_path_crate_template);
    let form_encoded = &config.rest_client.form_encoded_methods;
    let mut routes = Vec::new();
    for service in &plan.services {
        let handler = ServiceHandler {
            plan: service,
            metadata,
            config,
        };
        let service_name = format!("{}.{}", service.package, service.service_name);
        for method in handler.rest_methods() {
            let meta = &method.plan.metadata;
            let rpc = &meta.method_name;
            let http_method = &method.plan.http_method;
            let path = &meta.http_pattern.template;
            let whole_body = meta.http_rule.body == "*"
                && method.input_message().is_some()
                && method.plan.raw_request_body.is_none()
                && !form_encoded.contains(&format!("{service_name}.{rpc}"));
            let request = round_trip(
                whole_body
                    .then(|| message_path(&models, &meta.input_type))
                    .flatten(),
            );
            let answers = method.output_message().is_some()
                && !meta.server_streaming
                && method.plan.raw_response_body.is_none();
            let response = round_trip(
                answers
                    .then(|| message_path(&models, &meta.output_type))
                    .flatten(),
            );
            routes.push(quote! {
                Route {
                    service: #service_name,
                    rpc: #rpc,
                    method: #http_method,
                    path: #path,
                    request: #request,
                    response: #response,
                }
            });
        }
    }

    let tokens = quote! {
        /// The routed RPCs of the API: each HTTP route, with the JSON round trip of its request
        /// and response messages. A round trip reads a JSON body as the message and writes it
        /// back: what the models keep of what a server sends or accepts.
        pub mod routes {
            /// A JSON body read as a message and written back.
            pub type RoundTrip = fn(&[u8]) -> ::serde_json::Result<::serde_json::Value>;

            /// A routed RPC.
            pub struct Route {
                /// The service, as `package.Service`.
                pub service: &'static str,
                /// The RPC.
                pub rpc: &'static str,
                /// The HTTP method.
                pub method: &'static str,
                /// The path template of the route, such as `/widgets/{name}`.
                pub path: &'static str,
                /// The round trip of the request body, when the body is the whole request
                /// message.
                pub request: ::core::option::Option<RoundTrip>,
                /// The round trip of the response body, when the RPC answers with a message.
                pub response: ::core::option::Option<RoundTrip>,
            }

            fn round_trip<T: ::serde::Serialize + ::serde::de::DeserializeOwned>(
                body: &[u8],
            ) -> ::serde_json::Result<::serde_json::Value> {
                ::serde_json::to_value(::serde_json::from_slice::<T>(body)?)
            }

            /// The routes, in proto order.
            pub static ROUTES: &[Route] = &[#(#routes),*];
        }
    };
    format_tokens_static(tokens)
}

/// `Some(round_trip::<message>)`, or `None`.
fn round_trip(message: Option<syn::Path>) -> TokenStream {
    match message {
        Some(path) => quote! { ::core::option::Option::Some(round_trip::<#path>) },
        None => quote! { ::core::option::Option::None },
    }
}

/// The crate-local path of a message of the API, by its fully qualified proto name. `None` for
/// a well-known type, which is not a model.
fn message_path(models: &ModelsPath, message: &str) -> Option<syn::Path> {
    let package = message_package(message)?;
    if package.starts_with("google.") {
        return None;
    }
    let segment = package_models_segment(&package)?;
    let mut path = models.resolve(segment, package_models_version(&package));
    path.segments.push(extract_type_ident(message).into());
    Some(path)
}
