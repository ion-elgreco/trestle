// @generated — do not edit by hand.
#![allow(unexpected_cfgs)]
#![allow(clippy::empty_docs)]
#![allow(clippy::useless_borrows_in_formatting)]
#![allow(non_camel_case_types)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::doc_lazy_continuation)]

use std::collections::HashMap;

pub mod labels;
pub use labels::{ObjectLabel, Resource};

#[cfg(feature = "python")]
pub mod pyo3_impls;
#[cfg(feature = "python")]
pub use pyo3_impls::*;
pub use catalog::v1::Catalog;
pub use schemas::v1::Schema;

pub type PropertyMap = HashMap<String, serde_json::Value>;
pub mod catalog {
    pub mod v1 {
        include!("./example.catalog.v1.rs");
    }
}

pub mod schemas {
    pub mod v1 {
        include!("./example.schemas.v1.rs");
    }
}

pub mod tags {
    pub mod v1 {
        include!("./example.tags.v1.rs");
    }
}

#[cfg(feature = "axum")]
pub mod extractors;
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
    impl crate::models::schemas::v1::CreateSchemaRequest {
        /// Checks the `REQUIRED` fields.
        pub fn validate(&self) -> ::core::result::Result<(), ValidationError> {
            if self.r#name.is_empty() {
                return ::core::result::Result::Err(ValidationError::missing("name"));
            }
            if self.r#catalog_name.is_empty() {
                return ::core::result::Result::Err(
                    ValidationError::missing("catalog_name"),
                );
            }
            if self.r#schema_type.as_known().is_none_or(|v| v as i32 == 0) {
                return ::core::result::Result::Err(
                    ValidationError::missing("schema_type"),
                );
            }
            ::core::result::Result::Ok(())
        }
    }

    impl crate::models::schemas::v1::ListSchemasRequest {
        /// Checks the `REQUIRED` fields.
        pub fn validate(&self) -> ::core::result::Result<(), ValidationError> {
            if self.r#catalog_name.is_empty() {
                return ::core::result::Result::Err(
                    ValidationError::missing("catalog_name"),
                );
            }
            ::core::result::Result::Ok(())
        }
    }
}
