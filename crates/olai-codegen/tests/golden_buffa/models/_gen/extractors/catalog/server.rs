// @generated — do not edit by hand.
#![allow(unused_mut, unused_imports)]

use crate::models::catalog::v1::*;
use axum::{RequestExt, RequestPartsExt};

impl<S: Send + Sync> axum::extract::FromRequest<S> for CreateCatalogRequest {
    type Rejection = axum::response::Response;
    async fn from_request(
        req: axum::extract::Request<axum::body::Body>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::Json(request) = req
            .extract::<axum::Json<CreateCatalogRequest>, _>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for GetCatalogRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = GetCatalogRequest {
            name,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for ListCatalogsRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        #[derive(serde::Deserialize)]
        struct QueryParams {
            max_results: i32,
            page_token: String,
        }
        let axum_extra::extract::Query(QueryParams { max_results, page_token }) = parts
            .extract::<axum_extra::extract::Query<QueryParams>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = ListCatalogsRequest {
            max_results,
            page_token,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for UpdateCatalogRequest {
    type Rejection = axum::response::Response;
    async fn from_request(
        mut req: axum::extract::Request<axum::body::Body>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = req.into_parts();
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let body_req = axum::extract::Request::from_parts(parts, body);
        let axum::Json::<UpdateCatalogRequest>(body) = body_req
            .extract()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let catalog = body.catalog;
        let request = UpdateCatalogRequest {
            name,
            catalog,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for DeleteCatalogRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = DeleteCatalogRequest {
            name,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for GenerateCatalogTokenRequest {
    type Rejection = axum::response::Response;
    async fn from_request(
        req: axum::extract::Request<axum::body::Body>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::Json(request) = req
            .extract::<axum::Json<GenerateCatalogTokenRequest>, _>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for GetCatalogStatusRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let if_none_match = parts
            .headers
            .get("If-None-Match")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let request = GetCatalogStatusRequest {
            name,
            if_none_match,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for StreamCatalogEventsRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = StreamCatalogEventsRequest {
            name,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S>
for DownloadCatalogExportRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let range = parts
            .headers
            .get("Range")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let request = DownloadCatalogExportRequest {
            name,
            range,
            ..Default::default()
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for UploadCatalogImportRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(name) = parts
            .extract::<axum::extract::Path<String>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let contents = ::core::default::Default::default();
        let request = UploadCatalogImportRequest {
            name,
            contents,
            ..Default::default()
        };
        Ok(request)
    }
}
