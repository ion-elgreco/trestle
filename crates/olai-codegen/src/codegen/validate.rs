//! The `validate` module of the models: `validate()` on every message with a `REQUIRED` field
//! (`google.api.field_behavior`), for the generated extractors and the ConnectRPC bridge. A
//! required string, list or map must not be empty, a message must be set (and valid in turn),
//! an enum must not be its zero value. The generator appends the module to the models `mod.rs`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::config::ModelsPath;
use super::{
    CodeGenConfig, Runtime, extract_type_ident, format_tokens_static, message_package,
    package_models_segment, package_models_version,
};
use crate::google::api::FieldBehavior;
use crate::parsing::types::BaseType;
use crate::parsing::{CodeGenMetadata, MessageField, MessageInfo};

/// Whether the message (by its fully qualified name) has a `REQUIRED` field, and so a
/// `validate()`.
pub(crate) fn has_required_fields(metadata: &CodeGenMetadata, message: &str) -> bool {
    message_info(metadata, message).is_some_and(|info| info.fields.iter().any(is_required))
}

fn is_required(field: &MessageField) -> bool {
    field.field_behavior.contains(&FieldBehavior::Required)
}

/// The message, by its name with or without the leading dot.
fn message_info<'a>(metadata: &'a CodeGenMetadata, message: &str) -> Option<&'a MessageInfo> {
    metadata
        .messages
        .get(message)
        .or_else(|| metadata.messages.get(&format!(".{message}")))
}

/// The `validate` module, for the models `mod.rs`.
pub(crate) fn module(metadata: &CodeGenMetadata, config: &CodeGenConfig) -> String {
    let models = ModelsPath::from_template(&config.models_path_crate_template);
    let mut messages: Vec<(&String, &MessageInfo)> = metadata
        .messages
        .iter()
        .filter(|(_, info)| info.fields.iter().any(is_required))
        .collect();
    messages.sort_by(|a, b| a.0.cmp(b.0));
    let impls: Vec<TokenStream> = messages
        .iter()
        .filter_map(|(name, info)| {
            let path = message_path(&models, name)?;
            let checks = info
                .fields
                .iter()
                .filter(|field| is_required(field))
                .map(|field| check(field, metadata, config.runtime));
            Some(quote! {
                impl #path {
                    /// Checks the `REQUIRED` fields.
                    pub fn validate(&self) -> ::core::result::Result<(), ValidationError> {
                        #(#checks)*
                        ::core::result::Result::Ok(())
                    }
                }
            })
        })
        .collect();

    let tokens = quote! {
        /// The checks of the `REQUIRED` fields of the messages (`google.api.field_behavior`):
        /// `validate()` on every message that has one. A required string, list or map must not
        /// be empty, a message must be set and valid in turn, an enum must not be its zero
        /// value.
        pub mod validate {
            /// A required field that is missing.
            #[derive(Debug, Clone, PartialEq, Eq)]
            pub struct ValidationError {
                /// The field, by its proto name; a field of a nested message as
                /// `outer.inner`.
                pub field: ::std::string::String,
            }

            impl ValidationError {
                fn missing(field: &str) -> Self {
                    Self { field: field.into() }
                }

                fn nested(mut self, field: &str) -> Self {
                    self.field = format!("{field}.{}", self.field);
                    self
                }
            }

            impl ::core::fmt::Display for ValidationError {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    write!(f, "the field `{}` is required", self.field)
                }
            }

            impl ::std::error::Error for ValidationError {}

            #(#impls)*
        }
    };
    format_tokens_static(tokens)
}

/// The check of one required field: a `return Err(..)` when it is missing, and the validation
/// of a message field whose type has `validate()`.
fn check(field: &MessageField, metadata: &CodeGenMetadata, runtime: Runtime) -> TokenStream {
    let name = &field.name;
    let ident = format_ident!("r#{}", name);
    let ty = &field.unified_type;
    let missing = quote! {
        return ::core::result::Result::Err(ValidationError::missing(#name));
    };
    if ty.is_repeated || matches!(ty.base_type, BaseType::Map(..)) {
        return quote! { if self.#ident.is_empty() { #missing } };
    }
    match &ty.base_type {
        BaseType::String | BaseType::Bytes if ty.is_optional => quote! {
            if self.#ident.as_ref().is_none_or(|v| v.is_empty()) { #missing }
        },
        BaseType::String | BaseType::Bytes => quote! {
            if self.#ident.is_empty() { #missing }
        },
        BaseType::Message(message) => {
            let nested = has_required_fields(metadata, message).then(|| {
                quote! {
                    if let ::core::option::Option::Some(value) = value {
                        value.validate().map_err(|e| e.nested(#name))?;
                    }
                }
            });
            let value = match runtime {
                Runtime::Buffa => quote! { self.#ident.as_option() },
                Runtime::Prost => quote! { self.#ident.as_ref() },
            };
            quote! {
                let value = #value;
                if value.is_none() { #missing }
                #nested
            }
        }
        BaseType::Enum(_) => match (runtime, ty.is_optional) {
            (Runtime::Buffa, true) => quote! {
                if self.#ident.as_ref().and_then(|v| v.as_known()).is_none_or(|v| v as i32 == 0) {
                    #missing
                }
            },
            (Runtime::Buffa, false) => quote! {
                if self.#ident.as_known().is_none_or(|v| v as i32 == 0) { #missing }
            },
            (Runtime::Prost, true) => quote! {
                if self.#ident.is_none_or(|v| v == 0) { #missing }
            },
            (Runtime::Prost, false) => quote! {
                if self.#ident == 0 { #missing }
            },
        },
        // A number or a boolean is always set; a oneof has no single field.
        _ => quote! {},
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
