//! Serde helpers that write 64-bit integers as JSON numbers.
//!
//! Proto3 JSON writes `int64`/`uint64` (and their `sint`/`fixed` variants) as quoted decimal
//! strings, and buffa follows it: its models carry `with = "::buffa::json_helpers::int64"`. Many
//! REST APIs send and expect plain numbers. With
//! [`RestClientOptions::json_int64_as_number`](super::RestClientOptions::json_int64_as_number) the
//! generator appends a `json_int64` module to the models `mod.rs`, and the `trestle` CLI points
//! buffa's helper paths at it. Serialization writes a number; deserialization delegates to buffa,
//! so it still accepts numbers and strings.

use quote::quote;

use super::format_tokens_static;

/// The `json_int64` module appended to the models `mod.rs`.
pub(crate) fn module() -> String {
    let tokens = quote! {
        /// Serde helpers that write 64-bit integers as JSON numbers, not as the quoted strings of
        /// proto3 JSON. Reading accepts both.
        pub mod json_int64 {
            /// `i64` as a JSON number.
            pub mod int64 {
                pub fn serialize<S: ::serde::Serializer>(
                    value: &i64,
                    s: S,
                ) -> ::core::result::Result<S::Ok, S::Error> {
                    s.serialize_i64(*value)
                }

                pub fn deserialize<'de, D: ::serde::Deserializer<'de>>(
                    d: D,
                ) -> ::core::result::Result<i64, D::Error> {
                    ::buffa::json_helpers::int64::deserialize(d)
                }
            }

            /// `Option<i64>` as a JSON number or `null`.
            pub mod opt_int64 {
                pub fn serialize<S: ::serde::Serializer>(
                    value: &::core::option::Option<i64>,
                    s: S,
                ) -> ::core::result::Result<S::Ok, S::Error> {
                    match value {
                        ::core::option::Option::Some(v) => s.serialize_i64(*v),
                        ::core::option::Option::None => s.serialize_none(),
                    }
                }

                pub fn deserialize<'de, D: ::serde::Deserializer<'de>>(
                    d: D,
                ) -> ::core::result::Result<::core::option::Option<i64>, D::Error> {
                    ::buffa::json_helpers::opt_int64::deserialize(d)
                }
            }

            /// `u64` as a JSON number.
            pub mod uint64 {
                pub fn serialize<S: ::serde::Serializer>(
                    value: &u64,
                    s: S,
                ) -> ::core::result::Result<S::Ok, S::Error> {
                    s.serialize_u64(*value)
                }

                pub fn deserialize<'de, D: ::serde::Deserializer<'de>>(
                    d: D,
                ) -> ::core::result::Result<u64, D::Error> {
                    ::buffa::json_helpers::uint64::deserialize(d)
                }
            }

            /// `Option<u64>` as a JSON number or `null`.
            pub mod opt_uint64 {
                pub fn serialize<S: ::serde::Serializer>(
                    value: &::core::option::Option<u64>,
                    s: S,
                ) -> ::core::result::Result<S::Ok, S::Error> {
                    match value {
                        ::core::option::Option::Some(v) => s.serialize_u64(*v),
                        ::core::option::Option::None => s.serialize_none(),
                    }
                }

                pub fn deserialize<'de, D: ::serde::Deserializer<'de>>(
                    d: D,
                ) -> ::core::result::Result<::core::option::Option<u64>, D::Error> {
                    ::buffa::json_helpers::opt_uint64::deserialize(d)
                }
            }
        }
    };
    format_tokens_static(tokens)
}
