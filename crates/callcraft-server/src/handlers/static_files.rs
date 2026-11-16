use axum::Json;
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};

const INDEX_HTML: &str = include_str!("../../../../web/index.html");
const APP_JS: &str = include_str!("../../../../web/app.js");
const LIBRARY_JS: &str = include_str!("../../../../web/library.js");
const LIBRARY_CSS: &str = include_str!("../../../../web/library.css");
const AUTHOR_JS: &str = include_str!("../../../../web/author.js");
const SETTINGS_JS: &str = include_str!("../../../../web/settings.js");
const STYLES_CSS: &str = include_str!("../../../../web/styles.css");

pub(crate) async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub(crate) async fn app_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
        .into_response()
}

pub(crate) async fn library_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        LIBRARY_JS,
    )
        .into_response()
}

pub(crate) async fn library_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        LIBRARY_CSS,
    )
        .into_response()
}

pub(crate) async fn author_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        AUTHOR_JS,
    )
        .into_response()
}

pub(crate) async fn settings_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        SETTINGS_JS,
    )
        .into_response()
}

pub(crate) async fn styles() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLES_CSS,
    )
        .into_response()
}

pub(crate) async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true, "service": "callcraft-server"}))
}

pub(crate) async fn example() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true, "yaml": crate::EXAMPLE_SCENARIO}))
}
