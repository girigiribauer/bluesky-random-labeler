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

    async fn spawn_server() -> (std::net::SocketAddr, tokio::sync::broadcast::Sender<(i64, Vec<atrium_api::com::atproto::label::defs::Label>)>) {
        let pool = init_db(":memory:").await.unwrap();
        let mut rng = OsRng;
        let keypair = Arc::new(Secp256k1Keypair::create(&mut rng));
        let tx = tokio::sync::broadcast::channel(100).0;
        let state = AppState { pool, keypair, tx: tx.clone() };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
        (addr, tx)
    }

    async fn wait_receiver_count<T: Clone>(tx: &tokio::sync::broadcast::Sender<T>, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while tx.receiver_count() != expected {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("receiver_count stayed {} (expected {})", tx.receiver_count(), expected));
    }

    async fn connect(addr: std::net::SocketAddr) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
        let url = format!("ws://{}/xrpc/com.atproto.label.subscribeLabels", addr);
        tokio_tungstenite::connect_async(url).await.unwrap().0
    }

    /// クライアントが Close を送って切断したら、ラベルの配信がなくてもサーバー側の接続処理が終わること
    #[tokio::test]
    async fn test_subscribe_labels_ends_on_client_close() {
        let (addr, tx) = spawn_server().await;
        let mut ws = connect(addr).await;
        wait_receiver_count(&tx, 1).await;

        ws.close(None).await.unwrap();

        wait_receiver_count(&tx, 0).await;
    }

    /// クライアントが Close を送らずに TCP ごと切断しても、サーバー側の接続処理が終わること
    #[tokio::test]
    async fn test_subscribe_labels_ends_on_client_drop() {
        let (addr, tx) = spawn_server().await;
        let ws = connect(addr).await;
        wait_receiver_count(&tx, 1).await;

        drop(ws);

        wait_receiver_count(&tx, 0).await;
    }

    /// クライアントの Ping に Pong が返り、接続は保たれること
    #[tokio::test]
    async fn test_subscribe_labels_replies_pong() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (addr, tx) = spawn_server().await;
        let mut ws = connect(addr).await;
        wait_receiver_count(&tx, 1).await;

        ws.send(Message::Ping(b"hi".to_vec())).await.unwrap();
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(reply, Message::Pong(b"hi".to_vec()));
        assert_eq!(tx.receiver_count(), 1);
    }

    /// 接続中のクライアントには、ブロードキャストしたラベルが届くこと
    #[tokio::test]
    async fn test_subscribe_labels_delivers_broadcast() {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;

        let (addr, tx) = spawn_server().await;
        let mut ws = connect(addr).await;
        wait_receiver_count(&tx, 1).await;

        tx.send((42, vec![])).unwrap();
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert!(matches!(msg, Message::Binary(_)));
    }
}
