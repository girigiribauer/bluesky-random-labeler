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
        setup_app_with_pool().await.0
    }

    async fn setup_app_with_pool() -> (Router, crate::db::DbPool) {
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
        db_upsert(&pool, "did:plc:test2", "fortune_val", &now_str, false, "did:plc:labeler", false).await.unwrap();

        (router(state), pool)
    }

    /// 括弧つきの書き方で uriPatterns を渡したとき、その URI のラベルが値つきで返ること
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

    /// ラベルのない URI を問い合わせたとき、エラーにならず空の一覧が返ること
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

    async fn query_labels_status_and_count(uri: &str) -> (StatusCode, usize) {
        let app = setup_app().await;
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let response = app.oneshot(req).await.unwrap();
        let status = response.status();
        if status != StatusCode::OK {
            return (status, 0);
        }
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: QueryLabelsOutput = serde_json::from_slice(&body).unwrap();
        (status, body_json.labels.len())
    }

    /// uriPatterns を括弧なしで1つ渡したとき、200 でラベルが返ること
    #[tokio::test]
    async fn test_query_labels_single_plain_key() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:test",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 1);
    }

    /// uriPatterns を同じ名前の繰り返しで複数渡したとき、どの URI のラベルも漏れずに返ること
    #[tokio::test]
    async fn test_query_labels_repeated_key() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:test&uriPatterns=did:plc:test2",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 2);
    }

    /// URL 用に記号を変換した値（%3A など）を、元の URI に戻して問い合わせること
    #[tokio::test]
    async fn test_query_labels_percent_encoded_value() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did%3Aplc%3Atest",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 1);
    }

    /// 決まりにある sources を付けて問い合わせても、はじかずに 200 で返ること
    #[tokio::test]
    async fn test_query_labels_with_sources() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:test&sources=did:plc:labeler",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 1);
    }

    /// 上限ちょうどの limit=250 を付けても 200 で返ること
    #[tokio::test]
    async fn test_query_labels_limit_max() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:test&limit=250",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 1);
    }

    /// 括弧つきの書き方と limit を組み合わせても 200 で返ること
    #[tokio::test]
    async fn test_query_labels_bracket_key_with_limit() {
        let (status, count) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns[]=did:plc:test&limit=50",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(count, 1);
    }

    /// limit が上限を超えたときは 400 を返すこと
    #[tokio::test]
    async fn test_query_labels_limit_over_max() {
        let (status, _) = query_labels_status_and_count(
            "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:test&limit=251",
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    async fn get_query_labels(app: &Router, uri: &str) -> QueryLabelsOutput {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn setup_app_with_seven_labels() -> Router {
        let (app, pool) = setup_app_with_pool().await;
        let now_str = chrono::Utc::now().to_rfc3339();
        for val in ["daikichi", "kichi", "chukichi", "shokichi", "suekichi", "kyo", "daikyo"] {
            db_upsert(&pool, "did:plc:many", val, &now_str, false, "did:plc:labeler", false).await.unwrap();
        }
        app
    }

    /// limit を付けると、その件数までしか返さないこと
    #[tokio::test]
    async fn test_query_labels_limit_caps_count() {
        let app = setup_app_with_seven_labels().await;

        let out = get_query_labels(&app, "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:many&limit=3").await;

        assert_eq!(out.labels.len(), 3);
    }

    /// cursor をたどっていくと、その URI のラベルを重複なく全部取り出せること
    #[tokio::test]
    async fn test_query_labels_cursor_pages_through_all() {
        let app = setup_app_with_seven_labels().await;
        let base = "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:many&limit=3";

        let mut vals = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let uri = match &cursor {
                Some(c) => format!("{}&cursor={}", base, c),
                None => base.to_string(),
            };
            let out = get_query_labels(&app, &uri).await;
            if out.labels.is_empty() {
                break;
            }
            vals.extend(out.labels.iter().map(|l| l.val.clone()));
            cursor = out.cursor.clone();
            if cursor.is_none() {
                break;
            }
        }

        vals.sort();
        assert_eq!(vals, vec!["chukichi", "daikichi", "daikyo", "kichi", "kyo", "shokichi", "suekichi"]);
    }

    /// 複数の URI をまとめて問い合わせ、cursor をたどっていくと、どの URI のラベルも取りこぼさないこと
    #[tokio::test]
    async fn test_query_labels_cursor_pages_through_multiple_uris() {
        let app = setup_app_with_seven_labels().await;
        let base = "/xrpc/com.atproto.label.queryLabels?uriPatterns=did:plc:many&uriPatterns=did:plc:test&limit=3";

        let mut uris = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let uri = match &cursor {
                Some(c) => format!("{}&cursor={}", base, c),
                None => base.to_string(),
            };
            let out = get_query_labels(&app, &uri).await;
            if out.labels.is_empty() {
                break;
            }
            uris.extend(out.labels.iter().map(|l| l.uri.clone()));
            cursor = out.cursor.clone();
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(uris.iter().filter(|u| *u == "did:plc:many").count(), 7);
        assert_eq!(uris.iter().filter(|u| *u == "did:plc:test").count(), 1);
    }

    async fn post_report(app: &Router, reason: &str) {
        unsafe {
            std::env::set_var("LABELER_DID", "did:plc:test");
            std::env::set_var("SIGNING_KEY", "0000000000000000000000000000000000000000000000000000000000000000");
        }
        let payload = serde_json::json!({
            "reasonType": "com.atproto.moderation.defs#reasonOther",
            "reason": reason,
            "subject": {
                "$type": "com.atproto.repo.strongRef",
                "uri": "at://did:plc:target/app.bsky.feed.post/3juv3456789",
                "cid": "bafyreibnoelefnzgwbcacyt4vh52ymxvzbjq7mmqhtcnwarfq4lzegsiqe"
            }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/xrpc/com.atproto.moderation.createReport")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn positive_vals(pool: &crate::db::DbPool, did: &str) -> Vec<String> {
        crate::db::get_labels(pool, did, None, None).await.unwrap()
            .into_iter().filter(|l| l.neg == 0).map(|l| l.val).collect()
    }

    /// 理由におみくじの言葉を含む通報を受けると、通報された投稿の持ち主のおみくじがその言葉に書き換わること
    #[tokio::test]
    async fn test_create_report_overwrites_fortune() {
        let (app, pool) = setup_app_with_pool().await;

        post_report(&app, "今日は吉がいい").await;

        assert_eq!(positive_vals(&pool, "did:plc:target").await, vec!["kichi"]);
    }

    /// 理由に複数のおみくじの言葉が重なって含まれるときは、長いほうの言葉（「吉」より「大吉」）で書き換わること
    #[tokio::test]
    async fn test_create_report_prefers_longer_word() {
        let (app, pool) = setup_app_with_pool().await;

        post_report(&app, "大吉にして").await;

        assert_eq!(positive_vals(&pool, "did:plc:target").await, vec!["daikichi"]);
    }

    /// 理由におみくじの言葉がない通報では、おみくじを書き換えないこと
    #[tokio::test]
    async fn test_create_report_without_word_changes_nothing() {
        let (app, pool) = setup_app_with_pool().await;

        post_report(&app, "ただの通報です").await;

        assert!(positive_vals(&pool, "did:plc:target").await.is_empty());
    }

    /// 死活確認の窓口が 200 とバージョンを返すこと
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

    /// おみくじの言葉を含む通報を受け付けたとき、200 と通報の受付結果を返すこと
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
                "cid": "bafyreibnoelefnzgwbcacyt4vh52ymxvzbjq7mmqhtcnwarfq4lzegsiqe"
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
