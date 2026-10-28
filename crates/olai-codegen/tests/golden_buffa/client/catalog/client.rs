// @generated — do not edit by hand.
#![allow(unused_imports)]

use olai_http::CloudClient;
use url::Url;
use crate::Result;
use example_common::models::catalog::v1::*;
use super::super::encode_path_value;

/// HTTP client for service operations
#[derive(Clone)]
pub struct CatalogServiceClient {
    pub(crate) client: CloudClient,
    pub(crate) base_url: Url,
}

impl CatalogServiceClient {
    /// Create a new client instance
    pub fn new(client: CloudClient, mut base_url: Url) -> Self {
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Self { client, base_url }
    }

    /// A create answers 201.
    pub async fn create_catalog(
        &self,
        request: &CreateCatalogRequest,
    ) -> Result<Catalog> {
        let url = self.base_url.join("catalogs")?;
        let response = self.client.post(url).json(request).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    pub async fn get_catalog(&self, request: &GetCatalogRequest) -> Result<Catalog> {
        let formatted_path = format!(
            "catalogs/{}", encode_path_value(& request.name, false)
        );
        let url = self.base_url.join(&formatted_path)?;
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    pub async fn list_catalogs(
        &self,
        request: &ListCatalogsRequest,
    ) -> Result<ListCatalogsResponse> {
        let mut url = self.base_url.join("catalogs")?;
        url.query_pairs_mut()
            .append_pair("max_results", &request.max_results.to_string());
        url.query_pairs_mut().append_pair("page_token", &request.page_token);
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    pub async fn update_catalog(
        &self,
        request: &UpdateCatalogRequest,
    ) -> Result<Catalog> {
        let formatted_path = format!(
            "catalogs/{}", encode_path_value(& request.name, false)
        );
        let url = self.base_url.join(&formatted_path)?;
        let mut body = request.clone();
        body.name = Default::default();
        let response = self.client.patch(url).json(&body).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    pub async fn delete_catalog(
        &self,
        request: &DeleteCatalogRequest,
    ) -> Result<DeleteCatalogResponse> {
        let formatted_path = format!(
            "catalogs/{}", encode_path_value(& request.name, false)
        );
        let url = self.base_url.join(&formatted_path)?;
        let response = self.client.delete(url).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    /// Custom POST RPC without path params — covers `RequestType::Custom(Post)`
    /// dispatched as a collection method (the shape used by factory-style
    /// RPCs like `GenerateTemporary*Credentials`).
    pub async fn generate_catalog_token(
        &self,
        request: &GenerateCatalogTokenRequest,
    ) -> Result<CatalogToken> {
        let url = self.base_url.join("catalog-tokens")?;
        let response = self.client.post(url).json(request).send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let result = response.bytes().await?;
        Ok(serde_json::from_slice(&result)?)
    }

    /// Resource-targeted custom GET (path param, not a collection method) — exercises surfacing a
    /// custom read on the scoped client (`catalog.get_catalog_status()`) instead of leaving its
    /// generated builder orphaned.
    pub async fn get_catalog_status(
        &self,
        request: &GetCatalogStatusRequest,
    ) -> Result<CatalogStatus> {
        let formatted_path = format!(
            "catalogs/{}/status", encode_path_value(& request.name, false)
        );
        let url = self.base_url.join(&formatted_path)?;
        let mut builder = self.client.get(url);
        if let Some(ref value) = request.if_none_match {
            builder = builder.header("If-None-Match", value.as_str());
        }
        let response = builder.send().await?;
        if !response.status().is_success() {
            return Err(crate::error::parse_error_response(response).await);
        }
        let headers = response.headers().clone();
        let result = response.bytes().await?;
        let mut output: CatalogStatus = if result.trim_ascii().is_empty() {
            Default::default()
        } else {
            serde_json::from_slice(&result)?
        };
        if let Some(value) = headers.get("ETag").and_then(|value| value.to_str().ok()) {
            output.etag = value.to_string();
        }
        Ok(output)
    }
}
