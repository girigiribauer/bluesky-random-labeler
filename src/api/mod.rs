use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{get, post},
    Router,
};
use serde::de::DeserializeOwned;

pub mod label;
pub mod report;
pub mod websocket;
mod tests;

pub struct QsQuery<T>(pub T);

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/xrpc/com.atproto.label.queryLabels", get(label::query_labels))
        .route("/xrpc/com.atproto.label.subscribeLabels", get(websocket::subscribe_labels))
        .route("/xrpc/com.atproto.moderation.createReport", post(report::create_report))
        .route("/xrpc/_health", get(|| async { axum::Json(serde_json::json!({ "version": "0.0.0" })) }))
        .with_state(state)
}

#[async_trait]
impl<S, T> FromRequestParts<S> for QsQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        match serde_qs::from_str(query) {
            Ok(v) => Ok(QsQuery(v)),
            Err(e) => {
                eprintln!("serde_qs error: {}", e);
                Err(axum::http::StatusCode::BAD_REQUEST)
            }
        }
    }
}
