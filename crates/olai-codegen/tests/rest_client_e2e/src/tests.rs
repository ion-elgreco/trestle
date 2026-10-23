use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::TryStreamExt;
use serde_json::{Value, json};

use crate::codegen::DemoClient;
use crate::codegen::error::Error;
use crate::models::demo::v1::{Color, CreateWidgetRequest, ErrorCode, Widget};

/// What the server saw: the raw request paths, bodies, content types and authorization headers.
#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<String>>>);

impl Seen {
    fn push(&self, entry: impl Into<String>) {
        self.0.lock().unwrap().push(entry.into());
    }

    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

fn record(seen: &Seen, uri: &OriginalUri, headers: &HeaderMap) {
    seen.push(format!("path {}", uri.path()));
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        seen.push(format!("auth {}", auth.to_str().unwrap()));
    }
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        seen.push(format!("content-type {}", ct.to_str().unwrap()));
    }
}

async fn get_widget(
    State(seen): State<Seen>,
    uri: OriginalUri,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    record(&seen, &uri, &headers);
    match name.as_str() {
        "missing" => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({
                "error_code": "WIDGET_NOT_FOUND",
                "message": "no widget named missing",
                "details": [{"reason": "gone"}],
            })),
        )
            .into_response(),
        "odd" => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"errorCode": "SOMETHING_NEW", "message": "odd request"})),
        )
            .into_response(),
        "text" => (StatusCode::BAD_GATEWAY, "bad gateway\n").into_response(),
        _ => axum::Json(json!({
            "name": name,
            "color": "PURPLE",
            "created_at": 1_700_000_000_000_i64,
            "size_bytes": 12,
            "properties": {"a": "1"},
            "display_name": "Shown",
            "metadata-location": "s3://bucket/m",
        }))
        .into_response(),
    }
}

async fn create_widget(
    State(seen): State<Seen>,
    uri: OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    record(&seen, &uri, &headers);
    let body = String::from_utf8(body.to_vec()).unwrap();
    seen.push(format!("body {body}"));
    let request: Value = serde_json::from_str(&body).unwrap();
    axum::Json(json!({"name": request["name"], "created_at": request["created_at"]}))
        .into_response()
}

async fn list_widgets(State(seen): State<Seen>, uri: OriginalUri, headers: HeaderMap) -> Response {
    record(&seen, &uri, &headers);
    if let Some(query) = uri.query() {
        seen.push(format!("query {query}"));
    }
    let page = if uri.query().is_some_and(|q| q.contains("page_token=p2")) {
        json!({"widgets": [{"name": "w3"}]})
    } else {
        json!({"widgets": [{"name": "w1"}, {"name": "w2"}], "next_page_token": "p2"})
    };
    axum::Json(page).into_response()
}

async fn delete_widget(State(seen): State<Seen>, uri: OriginalUri, headers: HeaderMap) -> Response {
    record(&seen, &uri, &headers);
    StatusCode::OK.into_response()
}

async fn exchange_token(
    State(seen): State<Seen>,
    uri: OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    record(&seen, &uri, &headers);
    seen.push(format!(
        "body {}",
        String::from_utf8(body.to_vec()).unwrap()
    ));
    axum::Json(json!({"access_token": "issued", "expires_in": 3600})).into_response()
}

/// Start the demo server on a free port; returns its base URL and what it saw.
async fn serve() -> (url::Url, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .route("/api/v1/widgets", get(list_widgets).post(create_widget))
        .route(
            "/api/v1/widgets/{name}",
            get(get_widget).delete(delete_widget),
        )
        .route("/auth/tokens", post(exchange_token))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/").parse().unwrap(), seen)
}

#[tokio::test]
async fn path_values_are_percent_encoded() {
    let (base, seen) = serve().await;
    let client = DemoClient::new_unauthenticated(base);
    let widget = client.widget("a/b?c d").get().await.unwrap();
    assert_eq!(widget.name, "a/b?c d");
    assert!(
        seen.all()
            .contains(&"path /api/v1/widgets/a%2Fb%3Fc%20d".to_string()),
        "{:?}",
        seen.all()
    );
}

#[tokio::test]
async fn responses_read_numbers_names_and_unknown_enums() {
    let (base, _) = serve().await;
    let client = DemoClient::new_unauthenticated(base);
    let widget: Widget = client.widget("w").get().await.unwrap();
    // `PURPLE` is not a `Color`: with `ignore_unknown_enum_values` it becomes the zero value
    // instead of failing the response.
    assert_eq!(
        widget.color,
        buffa::EnumValue::Known(Color::COLOR_UNSPECIFIED)
    );
    assert_eq!(widget.created_at, Some(1_700_000_000_000));
    assert_eq!(widget.size_bytes, 12);
    assert_eq!(widget.display_name, "Shown");
    assert_eq!(widget.metadata_location, "s3://bucket/m");
    assert_eq!(widget.properties.get("a").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn requests_write_snake_case_and_int64_numbers() {
    let (base, seen) = serve().await;
    let client = DemoClient::new_unauthenticated(base);
    let created = client
        .create_widget("w")
        .with_created_at(1_700_000_000_000_i64)
        .with_display_name("Shown".to_string())
        .await
        .unwrap();
    assert_eq!(created.created_at, Some(1_700_000_000_000));
    let body = seen
        .all()
        .into_iter()
        .find_map(|e| e.strip_prefix("body ").map(str::to_string))
        .unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["created_at"], json!(1_700_000_000_000_i64));
    assert_eq!(body["display_name"], json!("Shown"));

    // Models write the same JSON outside the client.
    let json = serde_json::to_value(CreateWidgetRequest {
        name: "x".into(),
        created_at: Some(5),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(json["created_at"], json!(5));
    let widget = serde_json::to_value(Widget {
        name: "x".into(),
        size_bytes: 7,
        metadata_location: "m".into(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(widget["size_bytes"], json!(7));
    assert_eq!(widget["metadata-location"], json!("m"));
}

#[tokio::test]
async fn error_answers_keep_status_code_message_and_details() {
    let (base, _) = serve().await;
    let client = DemoClient::new_unauthenticated(base);

    let Error::Api(e) = client.widget("missing").get().await.unwrap_err() else {
        panic!("expected an API error");
    };
    assert_eq!(e.status, 404);
    assert_eq!(e.code.as_deref(), Some("WIDGET_NOT_FOUND"));
    assert_eq!(e.error_code(), Some(ErrorCode::WIDGET_NOT_FOUND));
    assert_eq!(e.message, "no widget named missing");
    assert_eq!(e.details, vec![json!({"reason": "gone"})]);
    assert_eq!(e.to_string(), "WIDGET_NOT_FOUND: no widget named missing");

    // A code that the client does not know is kept, from the camelCase key too.
    let err = client.widget("odd").get().await.unwrap_err();
    assert_eq!(err.status(), Some(400));
    assert_eq!(err.code(), Some("SOMETHING_NEW"));
    let Error::Api(e) = err else { unreachable!() };
    assert_eq!(e.error_code(), None);

    // A body that is not a JSON error becomes the message.
    let err = client.widget("text").get().await.unwrap_err();
    assert_eq!(err.status(), Some(502));
    assert_eq!(err.code(), None);
    assert_eq!(err.to_string(), "HTTP 502: bad gateway");
}

#[tokio::test]
async fn list_stream_reads_all_pages() {
    let (base, _) = serve().await;
    let client = DemoClient::new_unauthenticated(base);
    let page = client.list_widgets().await.unwrap();
    assert_eq!(page.widgets.len(), 2);
    let all: Vec<Widget> = client
        .list_widgets()
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    let names: Vec<_> = all.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["w1", "w2", "w3"]);
}

#[tokio::test]
async fn query_keys_are_explicit_json_names() {
    let (base, seen) = serve().await;
    let client = DemoClient::new_unauthenticated(base);
    client.list_widgets().with_first(3).await.unwrap();
    let seen = seen.all();
    assert!(seen.contains(&"query startIndex=3".to_string()), "{seen:?}");
}

#[tokio::test]
async fn bearer_token_goes_in_every_request_and_form_bodies_are_encoded() {
    let (base, seen) = serve().await;
    let client = DemoClient::new_with_token(base, "secret");
    client.widget("w").delete().await.unwrap();
    let token = client
        .exchange_token(
            "urn:ietf:params:oauth:grant-type:token-exchange",
            "id-token",
        )
        .await
        .unwrap();
    assert_eq!(token.access_token, "issued");
    assert_eq!(token.expires_in, 3600);
    let seen = seen.all();
    assert_eq!(
        seen.iter().filter(|e| *e == "auth Bearer secret").count(),
        2,
        "{seen:?}"
    );
    assert!(seen.contains(&"content-type application/x-www-form-urlencoded".to_string()));
    assert!(seen.contains(
        &"body grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange&subject_token=id-token"
            .to_string()
    ));

    // The token never shows in `Debug`.
    let transport = crate::codegen::transport::HttpClient::default().with_token("secret");
    assert!(!format!("{transport:?}").contains("secret"));
}
