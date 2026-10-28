use std::collections::HashMap;

use crate::gnostic::openapi::v3::Operation;
use crate::google::api::{FieldBehavior, HttpRule, ResourceDescriptor, ResourceReference};
use crate::parsing::http::HttpPattern;
use crate::parsing::types::{BaseType, UnifiedType};

/// Collected metadata for code generation
#[derive(Debug, Default)]
pub struct CodeGenMetadata {
    pub messages: HashMap<String, MessageInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub services: HashMap<String, ServiceInfo>,
}

impl CodeGenMetadata {
    /// Get message fields for a given type name
    pub fn get_message_fields(&self, type_name: &str) -> Vec<MessageField> {
        self.messages
            .get(type_name)
            .map(|msg| msg.fields.clone())
            .unwrap_or_default()
    }

    /// Find a resource descriptor whose `singular` field matches `name`.
    pub fn resource_from_singular(&self, name: &str) -> Option<&ResourceDescriptor> {
        self.messages.values().find_map(|info| {
            info.resource_descriptor
                .as_ref()
                .filter(|r| r.singular == name)
        })
    }

    /// Find a resource descriptor whose `plural` field matches `name`.
    pub fn resource_from_plural(&self, name: &str) -> Option<&ResourceDescriptor> {
        self.messages.values().find_map(|info| {
            info.resource_descriptor
                .as_ref()
                .filter(|r| r.plural == name)
        })
    }

    /// Get resource descriptor by message type name (simple or fully-qualified).
    pub fn get_resource_descriptor(&self, type_name: &str) -> Option<&ResourceDescriptor> {
        // Try direct lookup first (fully-qualified name)
        if let Some(descriptor) = self
            .messages
            .get(type_name)
            .and_then(|info| info.resource_descriptor.as_ref())
        {
            return Some(descriptor);
        }

        // Fall back to simple-name suffix match
        self.messages.iter().find_map(|(key, info)| {
            let simple = key.rfind('.').map(|i| &key[i + 1..]).unwrap_or(key);
            if simple == type_name {
                info.resource_descriptor.as_ref()
            } else {
                None
            }
        })
    }
}

/// Information about a protobuf message
#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub name: String,
    pub fields: Vec<MessageField>,
    pub resource_descriptor: Option<ResourceDescriptor>,
    pub documentation: Option<String>,
}

/// Information about a field in a protobuf message
#[derive(Debug, Clone)]
pub struct MessageField {
    pub name: String,
    /// The `json_name` that the proto writes out for the field; `None` for protoc's default
    /// lowerCamelCase name, even when an explicit name equals it.
    pub json_name: Option<String>,
    /// The HTTP response header that this response field comes from
    /// (`(olai.trestle.v1.response_header)`), instead of the body.
    pub response_header: Option<String>,
    /// The HTTP request header that this request field goes in
    /// (`(olai.trestle.v1.request_header)`), instead of the query or the body.
    pub request_header: Option<String>,
    /// Whether the field is sent even when unpopulated (`(olai.trestle.v1.emit_unpopulated)`).
    pub emit_unpopulated: bool,
    /// Whether the field is the raw HTTP body of its message (`(olai.trestle.v1.raw_body)`): the
    /// bytes of a request body, or of a response body, instead of JSON.
    pub raw_body: bool,
    /// Language-agnostic type; carries `is_optional` and `is_repeated` flags.
    pub unified_type: UnifiedType,
    pub documentation: Option<String>,
    /// For oneof fields, contains the variants with their field names and types.
    pub oneof_variants: Option<Vec<OneofVariant>>,
    /// Field behavior annotations from google.api.field_behavior
    pub field_behavior: Vec<FieldBehavior>,
    /// Whether the field is marked with `debug_redact = true` in the proto definition.
    /// Indicates the field contains sensitive data that should be routed to secret storage.
    pub is_sensitive: bool,
    /// Resource reference annotation from `google.api.resource_reference` (ext 1055).
    ///
    /// - `child_type` non-empty: this field identifies a parent container for the named resource
    ///   (e.g. `catalog_name` with `child_type = "unitycatalog.io/Schema"`).
    /// - `r#type` non-empty: this field directly identifies a resource of that type
    ///   (e.g. `full_name` with `type = "unitycatalog.io/Table"`).
    pub resource_reference: Option<ResourceReference>,
}

/// Information about a variant in a oneof field
#[derive(Debug, Clone)]
pub struct OneofVariant {
    pub field_name: String,      // e.g., "azure_service_principal"
    pub variant_name: String,    // e.g., "AzureServicePrincipal"
    pub field_type: UnifiedType, // the unified type (language-agnostic)
    pub documentation: Option<String>,
}

impl OneofVariant {
    /// Primitive-type oneofs use `i32` as the enum discriminant when crossing
    /// FFI boundaries (NAPI/Python extractors).  Return true when the variant
    /// holds a plain integer.
    pub fn is_int32(&self) -> bool {
        matches!(self.field_type.base_type, BaseType::Int32)
    }
}

/// Information about a protobuf enum
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub values: Vec<EnumValue>,
    pub documentation: Option<String>,
}

/// Information about an enum value
#[derive(Debug, Clone)]
pub struct EnumValue {
    pub name: String,
    pub number: i32,
    pub documentation: Option<String>,
}

/// Information about a protobuf service
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    /// Proto package name (e.g. `"unitycatalog.catalogs.v1"`).
    pub package: String,
    /// The service is served over ConnectRPC too (`(olai.trestle.v1.connect)`): the server
    /// mounts its Connect routes, and the clients call it over Connect when they prefer Connect.
    pub connect: bool,
    pub documentation: Option<String>,
    pub methods: Vec<MethodMetadata>,
}

/// Metadata extracted from a service method
#[derive(Debug, Clone)]
pub struct MethodMetadata {
    pub service_name: String,
    pub method_name: String,
    pub input_type: String,
    pub output_type: String,
    pub operation: Option<Operation>,
    pub http_rule: HttpRule,
    /// Pre-parsed HTTP URL pattern. Analysis should use this directly instead of re-parsing.
    pub http_pattern: HttpPattern,
    pub documentation: Option<String>,
    /// A server-streaming RPC (`returns (stream T)`): over REST, an NDJSON answer with one `T`
    /// per line.
    pub server_streaming: bool,
    /// `option idempotency_level = NO_SIDE_EFFECTS`: a read. Over Connect it may be a GET, and
    /// a server need not record it as a change.
    pub no_side_effects: bool,
    /// `(olai.trestle.v1.response_status)`: the HTTP status of the REST answer when it is not
    /// 200, such as 201 or 204. A 204 answer has no body.
    pub response_status: Option<u16>,
}

impl MethodMetadata {
    /// Return the HTTP method string (e.g. "GET", "POST").
    pub fn http_method(&self) -> Option<&str> {
        use crate::google::api::http_rule::Pattern;
        self.http_rule.pattern.as_ref().map(|p| match p {
            Pattern::Get(_) => "GET",
            Pattern::Post(_) => "POST",
            Pattern::Put(_) => "PUT",
            Pattern::Delete(_) => "DELETE",
            Pattern::Patch(_) => "PATCH",
            Pattern::Custom(c) => c.kind.as_str(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_based_method_structure() {
        let mut codegen_metadata = CodeGenMetadata {
            messages: HashMap::new(),
            services: HashMap::new(),
            enums: HashMap::new(),
        };

        // Create a test service with methods
        let mut service_info = ServiceInfo {
            name: "TestService".to_string(),
            package: "test.v1".to_string(),
            connect: false,
            documentation: Some("Test service documentation".to_string()),
            methods: Vec::new(),
        };

        // Add a method to the service
        let http_rule = crate::google::api::HttpRule {
            pattern: Some(crate::google::api::http_rule::Pattern::Get(
                "/test".to_string(),
            )),
            ..Default::default()
        };

        let method = MethodMetadata {
            service_name: "TestService".to_string(),
            method_name: "TestMethod".to_string(),
            input_type: ".test.TestRequest".to_string(),
            output_type: ".test.TestResponse".to_string(),
            server_streaming: false,
            no_side_effects: false,
            response_status: None,
            operation: None,
            http_pattern: crate::parsing::http::HttpPattern::parse("/test"),
            http_rule,
            documentation: Some("Test method documentation".to_string()),
        };
        service_info.methods.push(method);

        codegen_metadata
            .services
            .insert("TestService".to_string(), service_info);

        // Test that we can access the service and its methods directly
        let service = codegen_metadata.services.get("TestService").unwrap();
        assert_eq!(service.name, "TestService");
        assert_eq!(service.methods.len(), 1);
        assert_eq!(service.methods[0].method_name, "TestMethod");
    }
}
