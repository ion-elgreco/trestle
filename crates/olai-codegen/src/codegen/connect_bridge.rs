//! The ConnectRPC bridge of the server: for a service with `(olai.trestle.v1.connect)`, the
//! implementation of the service trait that connect-rust generates
//! ([`ServerOptions::connect_service_path_template`](super::ServerOptions::connect_service_path_template))
//! for any implementation of the handler trait of [`super::handler`], so that one handler serves
//! REST and Connect. The root `connect.rs` of the server output holds the bridge type and the
//! trait that reads the handler's context from a Connect request.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::config::ModelsPath;
use super::validate::has_required_fields;
use super::{Runtime, ServiceHandler, format_tokens};
use crate::{Error, Result};

/// The root `connect.rs` of the server output.
pub(super) fn root_module() -> String {
    let tokens = quote! {
        //! The ConnectRPC side of the handlers: a handler as a service of connect-rust
        //! ([`Bridge`]), with its context read from the Connect request
        //! ([`FromRequestContext`]). The `connect` module of each bridged service implements its
        //! service trait for the bridge, and registers the service on a `connectrpc::Router`.

        /// The context of a handler, from a ConnectRPC request: what `FromRequestParts` is to a
        /// REST request. The request's extensions carry what the app's layers put there.
        pub trait FromRequestContext: Sized {
            fn from_request_context(
                ctx: &::connectrpc::RequestContext,
            ) -> ::core::result::Result<Self, ::connectrpc::ConnectError>;
        }

        /// A handler as a ConnectRPC service.
        pub struct Bridge<H, Cx> {
            pub(crate) handler: H,
            context: ::core::marker::PhantomData<fn() -> Cx>,
        }

        impl<H, Cx> Bridge<H, Cx> {
            pub fn new(handler: H) -> Self {
                Self {
                    handler,
                    context: ::core::marker::PhantomData,
                }
            }
        }
    };
    format_tokens(tokens).expect("the bridge module is valid Rust")
}

/// The `connect.rs` of a service: the service trait for the bridge, `register` and the reads.
pub(super) fn generate(service: &ServiceHandler<'_>, template: &str) -> Result<String> {
    if service.config.runtime != Runtime::Buffa {
        return Err(Error::Build(
            "the ConnectRPC bridge needs the buffa runtime".into(),
        ));
    }
    let handler_trait = format_ident!("{}", service.plan.handler_name);
    let service_trait = format_ident!("{}", service.plan.service_name);
    let service_ext = format_ident!("{}Ext", service.plan.service_name);
    let connect_path =
        ModelsPath::new(template)?.resolve(&service.models_segment(), &service.models_version());
    let models_path = service.models_path();
    let foreign_models = service.foreign_models_imports();
    let procedure = |rpc: &str| {
        format!(
            "/{}.{}/{rpc}",
            service.plan.package, service.plan.service_name
        )
    };

    let server_methods = super::server::server_methods(service);
    let methods: Vec<TokenStream> = server_methods
        .iter()
        .map(|method| {
            let name = method.plan.base_method_ident();
            let input = method.input_type();
            let validation = has_required_fields(
                service.metadata,
                &method.plan.metadata.input_type,
            )
            .then(|| {
                quote! {
                    request
                        .validate()
                        .map_err(|e| ::connectrpc::ConnectError::invalid_argument(e.to_string()))?;
                }
            });
            let (output, call) = match method.output_type() {
                Some(output) => (
                    quote! { #output },
                    quote! {
                        let answer = self
                            .handler
                            .#name(request, context)
                            .await
                            .map_err(::core::convert::Into::<::connectrpc::ConnectError>::into)?;
                        ::core::result::Result::Ok(::connectrpc::Response::new(answer))
                    },
                ),
                None => (
                    quote! { ::buffa_types::google::protobuf::Empty },
                    quote! {
                        self.handler
                            .#name(request, context)
                            .await
                            .map_err(::core::convert::Into::<::connectrpc::ConnectError>::into)?;
                        ::core::result::Result::Ok(::connectrpc::Response::new(
                            ::core::default::Default::default(),
                        ))
                    },
                ),
            };
            quote! {
                #[allow(refining_impl_trait)]
                async fn #name(
                    &self,
                    ctx: ::connectrpc::RequestContext,
                    request: ::connectrpc::ServiceRequest<'_, #input>,
                ) -> ::connectrpc::ServiceResult<#output> {
                    let context = Cx::from_request_context(&ctx)?;
                    let request = request.to_owned_message();
                    #validation
                    #call
                }
            }
        })
        .collect();

    let reads: Vec<String> = server_methods
        .iter()
        .filter(|method| method.plan.metadata.no_side_effects)
        .map(|method| procedure(&method.plan.metadata.method_name))
        .collect();

    let doc = format!(
        "//! The ConnectRPC side of [`{}`]: the service trait of connect-rust for the bridge, \
         and the procedures that are reads.\n\n",
        service.plan.handler_name
    );
    let tokens = quote! {
        use #models_path::*;
        #(#foreign_models)*
        use #connect_path::{#service_trait, #service_ext};
        use super::handler::#handler_trait;
        use super::super::connect::{Bridge, FromRequestContext};

        impl<H, Cx> #service_trait for Bridge<H, Cx>
        where
            H: #handler_trait<Cx> + Clone + Send + Sync + 'static,
            Cx: FromRequestContext + Send + Sync + 'static,
        {
            #(#methods)*
        }

        /// The Connect routes of the service on `router`, with `handler` behind them.
        pub fn register<H, Cx>(handler: H, router: ::connectrpc::Router) -> ::connectrpc::Router
        where
            H: #handler_trait<Cx> + Clone + Send + Sync + 'static,
            Cx: FromRequestContext + Send + Sync + 'static,
        {
            #service_ext::register(::std::sync::Arc::new(Bridge::new(handler)), router)
        }

        /// The procedures that are reads (`idempotency_level = NO_SIDE_EFFECTS`), as
        /// `/package.Service/Rpc`.
        pub const READ_ONLY_PROCEDURES: &[&str] = &[#(#reads),*];
    };
    Ok(format!("{doc}{}", format_tokens(tokens)?))
}
