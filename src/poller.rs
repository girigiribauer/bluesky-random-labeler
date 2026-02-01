use anyhow::Result;
use atrium_api::agent::atp_agent::AtpAgent;
use atrium_xrpc_client::reqwest::ReqwestClient;
use std::time::Duration;
use tokio::time::sleep;
use crate::config::config;
use crate::db::DbPool;
use atrium_crypto::keypair::Secp256k1Keypair;
use tokio::sync::broadcast;
use atrium_api::com::atproto::label::defs::Label;
use crate::domain::labeling::assign_fortune;
use std::sync::Arc;
use atrium_api::agent::atp_agent::store::MemorySessionStore;

pub async fn start_polling(
    pool: DbPool,
    keypair: Arc<Secp256k1Keypair>,
    tx: broadcast::Sender<(i64, Vec<Label>)>
) -> Result<()> {
    let conf = config();
    let agent = AtpAgent::new(ReqwestClient::new("https://bsky.social"), MemorySessionStore::default());

    if let Some(pwd) = &conf.labeler_password {
        let identifier = conf.handle.as_deref().unwrap_or(&conf.labeler_did);
        tracing::info!(identifier, "Attempting login");
        agent.login(identifier, pwd).await?;
        tracing::info!("Bot logged in");
    } else {
        tracing::warn!("No password provided, skipping bot login (polling will fail).");
        return Ok(());
    }

    let mut last_seen_at: Option<String> = None;

    loop {
        match check_notifications(&agent, &pool, &keypair, &last_seen_at, &tx).await {
            Ok(new_last_seen) => {
                if let Some(t) = new_last_seen {
                    last_seen_at = Some(t);
                }
            }
            Err(_e) => {
            }
        }
        sleep(Duration::from_secs(10)).await;
    }
}

async fn check_notifications(
    agent: &AtpAgent<MemorySessionStore, ReqwestClient>,
    pool: &DbPool,
    keypair: &Secp256k1Keypair,
    last_seen_at: &Option<String>,
    tx: &broadcast::Sender<(i64, Vec<Label>)>
) -> Result<Option<String>> {
    let limit: i32 = 50;
    let resp = agent.api.app.bsky.notification.list_notifications(
         atrium_api::app::bsky::notification::list_notifications::ParametersData {
             cursor: None,
             limit: Some((limit as u8).try_into().unwrap()),
             priority: None,
             reasons: None,
             seen_at: None,
         }.into()
    ).await?;

    let mut max_indexed_at = last_seen_at.clone();

    for notif in &resp.notifications {
        let indexed_at = notif.indexed_at.as_str().to_string();
        if max_indexed_at.is_none() || indexed_at > max_indexed_at.as_ref().unwrap().clone() {
            max_indexed_at = Some(indexed_at.clone());
        }

        if let Some(last) = last_seen_at {
            if &indexed_at <= last {
                continue;
            }
        }

        match notif.reason.as_str() {
            "follow" | "like" => {
                let did = &notif.author.did;
                let handle = notif.author.handle.as_str();

                assign_fortune(did.as_str(), Some(handle), pool, keypair, &config().labeler_did, tx).await?;
            }
             _ => {}
        }
    }

    if let Some(t) = &max_indexed_at {
         let dt = chrono::DateTime::parse_from_rfc3339(t)?;
         agent.api.app.bsky.notification.update_seen(
             atrium_api::app::bsky::notification::update_seen::InputData {
                 seen_at: atrium_api::types::string::Datetime::new(dt),
             }.into()
         ).await?;
    }

    Ok(max_indexed_at)
}
