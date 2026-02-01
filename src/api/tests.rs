#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use tower::util::ServiceExt;
    use serde_json::Value;
    use atrium_api::com::atproto::label::query_labels::Output as QueryLabelsOutput;
    use atrium_api::com::atproto::moderation::create_report::Output as ReportOutput;
    use crate::api::router;
    use crate::state::AppState;
    use crate::db::init_db;
    use crate::db::upsert_label as db_upsert;
    use atrium_crypto::keypair::Secp256k1Keypair;
    use std::sync::Arc;
    use rand::rngs::OsRng;

    async fn setup_app() -> Router {
        let pool = init_db(":memory:").await.unwrap();
        let mut rng = OsRng;
        let keypair = Arc::new(Secp256k1Keypair::create(&mut rng));
        let state = AppState {
            pool: pool.clone(),
            keypair: keypair.clone(),
            tx: tokio::sync::broadcast::channel(100).0,
        };

        // Pre-insert some data
        let now_str = chrono::Utc::now().to_rfc3339();
        db_upsert(&pool, "did:plc:test", "fortune_val", &now_str, false, "did:plc:labeler", false).await.unwrap();

        router(state)
    }

    #[tokio::test]
    async fn test_query_labels() {
        let app = setup_app().await;

        let req = Request::builder()
            .uri("/xrpc/com.atproto.label.queryLabels?uriPatterns[]=did:plc:test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        // println!("BODY: {}", body_str);

        let body_json: QueryLabelsOutput = serde_json::from_str(&body_str).unwrap();

        assert_eq!(body_json.labels.len(), 1);
        assert_eq!(body_json.labels[0].uri, "did:plc:test");
        assert_eq!(body_json.labels[0].val, "fortune_val");
    }

    #[tokio::test]
    async fn test_query_labels_empty() {
        let app = setup_app().await;

        let req = Request::builder()
            .uri("/xrpc/com.atproto.label.queryLabels?uriPatterns[]=did:plc:unknown")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: QueryLabelsOutput = serde_json::from_slice(&body).unwrap();

        assert_eq!(body_json.labels.len(), 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = setup_app().await;

        let req = Request::builder()
            .uri("/xrpc/_health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body_json["version"], "0.0.0");
    }

    #[tokio::test]
    async fn test_create_report() {
        // Ensure config is loaded or env vars set (Mocking env for test safety if not loaded)
        unsafe {
            std::env::set_var("LABELER_DID", "did:plc:test");
            std::env::set_var("SIGNING_KEY", "0000000000000000000000000000000000000000000000000000000000000000");
        }

        let app = setup_app().await;

        let payload = serde_json::json!({
            "reasonType": "com.atproto.moderation.defs#reasonSpam",
            "reason": "Test report with keyword: daikichi",
            "subject": {
                "$type": "com.atproto.repo.strongRef",
                "uri": "at://did:plc:target/app.bsky.feed.post/3juv3456789",
                "cid": "bafyreihT00000000000000000000000000000000000000000000000000"
            }
        });

        let req = Request::builder()
            .method("POST")
            .uri("/xrpc/com.atproto.moderation.createReport")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();

        // Should be 200 OK even if logic runs
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: ReportOutput = serde_json::from_slice(&body).unwrap(); // Output matches createReport response type
        assert_eq!(body_json.data.id, 12345);
    }
}
