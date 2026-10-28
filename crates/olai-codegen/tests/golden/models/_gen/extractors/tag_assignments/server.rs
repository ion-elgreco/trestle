// @generated — do not edit by hand.
#![allow(unused_mut, unused_imports)]

use crate::models::tags::v1::*;
use axum::{RequestExt, RequestPartsExt};

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for ListTagAssignmentsRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path((entity_type, entity_name)) = parts
            .extract::<axum::extract::Path<(String, String)>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        #[derive(serde::Deserialize)]
        struct QueryParams {
            max_results: i32,
            page_token: String,
        }
        let axum_extra::extract::Query(QueryParams { max_results, page_token }) = parts
            .extract::<axum_extra::extract::Query<QueryParams>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = ListTagAssignmentsRequest {
            entity_type,
            entity_name,
            max_results,
            page_token,
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for CreateTagAssignmentRequest {
    type Rejection = axum::response::Response;
    async fn from_request(
        mut req: axum::extract::Request<axum::body::Body>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = req.into_parts();
        let axum::extract::Path((entity_type, entity_name)) = parts
            .extract::<axum::extract::Path<(String, String)>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let body_req = axum::extract::Request::from_parts(parts, body);
        let axum::Json::<TagAssignment>(body) = body_req
            .extract()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let tag = body.into();
        let request = CreateTagAssignmentRequest {
            entity_type,
            entity_name,
            tag,
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for GetTagAssignmentRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path((entity_type, entity_name, tag_key)) = parts
            .extract::<axum::extract::Path<(String, String, String)>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = GetTagAssignmentRequest {
            entity_type,
            entity_name,
            tag_key,
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for DeleteTagAssignmentRequest {
    type Rejection = axum::response::Response;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::extract::Path((entity_type, entity_name, tag_key)) = parts
            .extract::<axum::extract::Path<(String, String, String)>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = DeleteTagAssignmentRequest {
            entity_type,
            entity_name,
            tag_key,
        };
        Ok(request)
    }
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for TouchTagAssignmentRequest {
    type Rejection = axum::response::Response;
    async fn from_request(
        mut req: axum::extract::Request<axum::body::Body>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let (mut parts, _body) = req.into_parts();
        let axum::extract::Path((entity_type, entity_name, tag_key)) = parts
            .extract::<axum::extract::Path<(String, String, String)>>()
            .await
            .map_err(axum::response::IntoResponse::into_response)?;
        let request = TouchTagAssignmentRequest {
            entity_type,
            entity_name,
            tag_key,
        };
        Ok(request)
    }
}
