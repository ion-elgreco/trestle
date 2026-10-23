use std::ffi::CString;

use axum::Router;
use axum::extract::{OriginalUri, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::json;

async fn get_widget(Path(name): Path<String>) -> Response {
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
        "green" => axum::Json(json!({"name": name, "color": "GREEN"})).into_response(),
        "json" => axum::Json(json!({
            "name": name,
            "attributes": {"count": 3, "ratio": 0.5, "nested": {"id": 7}},
            "extra": [1, "two", null, true],
            "notes": [{"a": 1}, 2.5],
            "rows": {"t": [1, 2]},
        }))
        .into_response(),
        _ => axum::Json(json!({
            "name": name,
            "color": "PURPLE",
            "created_at": 1_700_000_000_000_i64,
            "properties": {"a": "1"},
        }))
        .into_response(),
    }
}

async fn list_widgets(uri: OriginalUri) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = if query.contains("color=GREEN") {
        json!({"widgets": [{"name": "g1", "color": "GREEN"}]})
    } else if query.contains("page_token=p2") {
        json!({"widgets": [{"name": "w3"}]})
    } else {
        json!({"widgets": [{"name": "w1"}, {"name": "w2"}], "next_page_token": "p2"})
    };
    axum::Json(page).into_response()
}

async fn create_widget(body: String) -> Response {
    let request: serde_json::Value = serde_json::from_str(&body).unwrap();
    // The raw body comes back as the display name, so the test sees its numbers.
    axum::Json(json!({
        "name": request["name"],
        "created_at": request["created_at"],
        "attributes": request["attributes"],
        "display_name": body,
        "type": request["type"],
    }))
    .into_response()
}

async fn widget_exists(Path(name): Path<String>) -> StatusCode {
    if name == "missing" {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::OK
    }
}

/// A `GET` with a JSON body: the widgets whose names start with `prefix`.
async fn search_widgets(axum::Json(body): axum::Json<serde_json::Value>) -> Response {
    let prefix = body["prefix"].as_str().unwrap_or_default().to_string();
    let widgets: Vec<_> = ["w1", "w2", "x1"]
        .into_iter()
        .filter(|name| name.starts_with(&prefix))
        .map(|name| json!({"name": name}))
        .collect();
    axum::Json(json!({"widgets": widgets})).into_response()
}

/// An NDJSON answer: one widget per line.
async fn stream_widgets() -> Response {
    let lines: String = ["w1", "w2"]
        .into_iter()
        .map(|name| format!("{}\n", json!({"name": name})))
        .collect();
    (
        [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
        lines,
    )
        .into_response()
}

/// A version in the response headers, the name in the body.
async fn widget_version(Path(name): Path<String>) -> Response {
    let body = json!({"name": name}).to_string();
    ([("Widget-Version", "7"), ("ETag", "abc")], body).into_response()
}

/// Start the demo server on its own Tokio runtime; returns its base URL and the runtime, which
/// must stay alive while the test runs.
fn serve() -> (String, tokio::runtime::Runtime) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let base = runtime.block_on(async {
        let app = Router::new()
            .route("/api/v1/widgets", get(list_widgets).post(create_widget))
            .route(
                "/api/v1/widgets/{name}",
                get(get_widget)
                    .delete(|| async { StatusCode::OK })
                    .head(widget_exists),
            )
            .route("/api/v1/search", get(search_widgets))
            .route("/api/v1/widget-lines", get(stream_widgets))
            .route("/api/v1/widgets/{name}/version", get(widget_version));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/")
    });
    (base, runtime)
}

/// Run `code` with `BASE` (the server URL) and the generated package importable: `demo` from
/// the package directory, `demo._core` from the generated `register`.
fn run_python(code: &str) {
    let (base, _server) = serve();
    Python::initialize();
    Python::attach(|py| {
        let result = (|| -> PyResult<()> {
            let core = PyModule::new(py, "_core")?;
            crate::codegen::register(&core)?;
            let sys = py.import("sys")?;
            sys.getattr("modules")?.set_item("demo._core", &core)?;
            sys.getattr("path")?
                .call_method1("insert", (0, crate::PACKAGE_DIR))?;
            let globals = pyo3::types::PyDict::new(py);
            globals.set_item("BASE", &base)?;
            py.run(&CString::new(code).unwrap(), Some(&globals), None)
        })();
        if let Err(e) = result {
            let traceback = e
                .traceback(py)
                .and_then(|t| t.format().ok())
                .unwrap_or_default();
            panic!("{traceback}{e}");
        }
    });
}

#[test]
fn python_api() {
    run_python(
        r#"
import asyncio
import pickle

from demo._core import DemoClient, DemoError, NotFoundError, ServerError
from demo._core.demo import Label, Widget, WidgetClient
from demo._enums import Color

client = DemoClient(BASE)

# Module paths.
assert DemoClient.__module__ == "demo._core", DemoClient.__module__
assert Widget.__module__ == "demo._core.demo", Widget.__module__
assert WidgetClient.__module__ == "demo._core.demo", WidgetClient.__module__

# Frozen models with a Python repr; an unknown enum value reads as None.
w = client.widget("a/b c").get()
assert w.name == "a/b c"
assert w.color is None, w.color
assert w.created_at == 1700000000000
assert w.properties == {"a": "1"}
assert repr(w) == (
    "Widget(name='a/b c', created_at=1700000000000, size_bytes=0, "
    "properties={'a': '1'}, display_name='', metadata_location='', notes=[], rows={}, "
    "sizes=[], counts={})"
), repr(w)
try:
    w.name = "x"
    raise AssertionError("models are frozen")
except AttributeError:
    pass

# Enum members are their names as str; parameters take a member or a name.
g = client.widget("green").get()
assert g.color is Color.GREEN
assert g.color == "GREEN" and isinstance(g.color, str) and str(g.color) == "GREEN"
assert f"{g.color}" == "GREEN"
assert [x.name for x in client.list_widgets(color="GREEN")] == ["g1"]
assert [x.name for x in client.list_widgets(color=Color.GREEN)] == ["g1"]
assert not hasattr(Color, "COLOR_UNSPECIFIED")
try:
    client.list_widgets(color="BLUE")
    raise AssertionError("unknown enum names are rejected")
except ValueError as e:
    assert "expected one of: RED, GREEN" in str(e), e

# Lists read every page.
assert [x.name for x in client.list_widgets()] == ["w1", "w2", "w3"]

# Only models that a request takes have a constructor; optional arguments are keyword-only.
label = Label(key="k", value="v")
assert label.key == "k"
try:
    Widget(name="x")
    raise AssertionError("a response model has no constructor")
except TypeError:
    pass
created = client.create_widget("n", created_at=5, labels=[label])
assert created.created_at == 5
try:
    client.create_widget("n", 5)
    raise AssertionError("optional arguments are keyword-only")
except TypeError:
    pass

# Error answers raise the exception class of their status.
try:
    client.widget("missing").get()
    raise AssertionError("expected an error")
except NotFoundError as e:
    assert isinstance(e, DemoError) and isinstance(e, Exception)
    assert type(e).__module__ == "demo._core", type(e).__module__
    assert e.code == "WIDGET_NOT_FOUND" and e.status == 404, (e.code, e.status)
    assert e.details == [{"reason": "gone"}], e.details
    assert str(e) == "WIDGET_NOT_FOUND: no widget named missing", str(e)
    assert repr(e) == "NotFoundError('no widget named missing', code='WIDGET_NOT_FOUND', status=404)", repr(e)
    # Pickle keeps the class and the fields, as multiprocessing needs.
    again = pickle.loads(pickle.dumps(e))
    assert type(again) is NotFoundError and str(again) == str(e) and again.details == e.details
    assert e.args == ("no widget named missing",), e.args
# Users can raise them too.
try:
    raise ServerError("down", status=503)
except DemoError as e:
    assert str(e) == "HTTP 503: down" and e.details == [], (str(e), e.details)
    assert e.args == ("down",), e.args
try:
    DemoClient("http://127.0.0.1:9/").widget("w").get()
    raise AssertionError("expected an error")
except DemoError as e:
    assert type(e) is DemoError and e.status is None and e.code is None

# Free-form JSON fields are plain Python objects; whole numbers are ints both ways.
w = client.widget("json").get()
assert w.attributes == {"count": 3, "ratio": 0.5, "nested": {"id": 7}}, w.attributes
assert type(w.attributes["count"]) is int
assert w.extra == [1, "two", None, True], w.extra
assert w.notes == [{"a": 1}, 2.5], w.notes
assert w.rows == {"t": [1, 2]}, w.rows
created = client.create_widget("w2", attributes={"count": 3, "flag": True, "list": [1, 2.5]})
assert created.attributes == {"count": 3, "flag": True, "list": [1, 2.5]}, created.attributes
assert '"count":3' in created.display_name, created.display_name
# A field named after a Rust keyword keeps its name in Python.
assert client.create_widget("w4", type="gear").type == "gear"
try:
    client.create_widget("w3", attributes={"bad": object()})
    raise AssertionError("expected a TypeError")
except TypeError:
    pass

# A HEAD request gives None, or raises for a missing widget; a GET can carry a JSON body.
assert client.widget_exists("w") is None
try:
    client.widget_exists("missing")
    raise AssertionError("expected an error")
except NotFoundError:
    pass
assert [w.name for w in client.search_widgets("w", ["a"]).widgets] == ["w1", "w2"]

# A streaming method gives a list, one item per NDJSON line; fields can come from headers.
assert [w.name for w in client.stream_widgets()] == ["w1", "w2"]
version = client.widget_version("w")
assert (version.name, version.version, version.etag) == ("w", 7, "abc"), version

# Each method has an `_async` twin.
async def main():
    w = await client.widget("w").get_async()
    assert w.name == "w"
    assert await client.widget("w").delete_async() is None
    names = [x.name for x in await client.list_widgets_async()]
    assert names == ["w1", "w2", "w3"], names
    try:
        await client.widget("missing").get_async()
        raise AssertionError("expected an error")
    except NotFoundError:
        pass

asyncio.run(main())
"#,
    );
}

/// The generated classes carry `pyo3-stub-gen` annotations: the stubs show the module layout,
/// the proto docs, the `_async` futures and the `str` enums.
#[test]
fn stubs() {
    let dir = std::path::PathBuf::from(concat!(env!("OUT_DIR"), "/stubs"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyo3_stub_gen::StubInfo::from_project_root(
        "demo._core".into(),
        dir.clone(),
        true,
        Default::default(),
    )
    .unwrap()
    .generate()
    .unwrap();
    let root = std::fs::read_to_string(dir.join("demo/_core/__init__.pyi")).unwrap();
    let models = std::fs::read_to_string(dir.join("demo/_core/demo/__init__.pyi")).unwrap();
    for expected in [
        "class DemoClient:",
        "def list_widgets(self, *, max_results: typing.Optional[builtins.int] = None, \
         color: typing.Optional[_enums.Color | str] = None, first: typing.Optional[builtins.int] = None, \
         order_by: typing.Optional[builtins.str] = None) -> builtins.list[demo.Widget]:",
        "-> asyncio.Future[builtins.list[demo.Widget]]:",
        "def widget(self, widget_name: builtins.str) -> demo.WidgetClient:",
        "class DemoError(builtins.Exception):",
        "class NotFoundError(DemoError):",
        "def details(self) -> list[typing.Any]:",
    ] {
        assert!(root.contains(expected), "missing `{expected}` in:\n{root}");
    }
    for expected in [
        "class Widget:",
        "def color(self) -> typing.Optional[_enums.Color]:",
        // A field named after a Rust keyword keeps its Python name.
        "def type(self) -> typing.Optional[builtins.str]:",
        "The color of the widget.",
        "def __new__(cls, key: typing.Optional[builtins.str] = None",
        "def get_async(self) -> asyncio.Future[Widget]:",
        "def delete_async(self) -> asyncio.Future[None]:",
        "def attributes(self) -> typing.Optional[dict[str, typing.Any]]:",
        "def extra(self) -> typing.Any:",
        "def notes(self) -> list[typing.Any]:",
        "def rows(self) -> dict[str, list[typing.Any]]:",
    ] {
        assert!(
            models.contains(expected),
            "missing `{expected}` in:\n{models}"
        );
    }
    // Request messages are not Python classes, and response models have no constructor.
    assert!(!models.contains("class CreateWidgetRequest"), "{models}");
    let widget = &models[models.find("class Widget:").unwrap()..];
    let widget = &widget[..widget.find("class WidgetClient:").unwrap()];
    assert!(!widget.contains("__new__"), "{widget}");
}
