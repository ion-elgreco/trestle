// @generated — do not edit by hand.
#![allow(unused_mut, clippy::too_many_arguments)]

use crate::Result;
use example_common::models::tags::v1::*;
use super::handler::TagAssignmentHandler;
use axum::extract::State;

pub async fn list_tag_assignments<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: ListTagAssignmentsRequest,
) -> Result<axum::Json<ListTagAssignmentsResponse>>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.list_tag_assignments(request, context).await?;
    Ok(axum::Json(result))
}

pub async fn create_tag_assignment<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: CreateTagAssignmentRequest,
) -> Result<axum::Json<TagAssignment>>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.create_tag_assignment(request, context).await?;
    Ok(axum::Json(result))
}

pub async fn get_tag_assignment<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: GetTagAssignmentRequest,
) -> Result<axum::Json<TagAssignment>>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.get_tag_assignment(request, context).await?;
    Ok(axum::Json(result))
}

pub async fn delete_tag_assignment<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: DeleteTagAssignmentRequest,
) -> Result<axum::Json<DeleteTagAssignmentResponse>>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    let result = handler.delete_tag_assignment(request, context).await?;
    Ok(axum::Json(result))
}

pub async fn touch_tag_assignment<T, Cx>(
    State(handler): State<T>,
    context: Cx,
    request: TouchTagAssignmentRequest,
) -> Result<()>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send,
{
    handler.touch_tag_assignment(request, context).await?;
    Ok(())
}

/// The routes of the service with their handlers, for an app to merge: the app's
/// state (`T`) implements the handler trait, and `Cx` is the context of its requests.
pub fn router<T, Cx>() -> axum::Router<T>
where
    T: TagAssignmentHandler<Cx> + Clone + Send + Sync + 'static,
    Cx: axum::extract::FromRequestParts<T> + Send + 'static,
{
    axum::Router::new()
        .route(
            "/entities/{entity_type}/{entity_name}/tags",
            axum::routing::get(list_tag_assignments::<T, Cx>)
                .post(create_tag_assignment::<T, Cx>),
        )
        .route(
            "/entities/{entity_type}/{entity_name}/tags/{tag_key}",
            axum::routing::get(get_tag_assignment::<T, Cx>)
                .delete(delete_tag_assignment::<T, Cx>),
        )
        .route(
            "/entities/{entity_type}/{entity_name}/tags/{tag_key}:touch",
            axum::routing::post(touch_tag_assignment::<T, Cx>),
        )
}
