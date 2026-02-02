use anyhow::Result;
use atrium_api::agent::atp_agent::AtpAgent;
use atrium_xrpc_client::reqwest::ReqwestClient;
use crate::config::config;
use crate::db::DbPool;
use crate::domain::labeling::{assign_fortune, revoke_fortune, overwrite_fortune};
use crate::domain::fortune::Fortune;
use std::str::FromStr;
use crate::crypto::create_keypair;

use sqlx::Row;

use atrium_api::agent::atp_agent::store::MemorySessionStore;

use tokio::sync::broadcast;
use atrium_api::com::atproto::label::defs::Label;

use std::sync::Arc;
use tracing;

pub async fn run_optimized_batch(pool: DbPool, tx: broadcast::Sender<(i64, Vec<Label>)>) -> Result<()> {
    tracing::info!("Running optimized batch");
    let conf = config();
    let agent = AtpAgent::new(ReqwestClient::new("https://bsky.social"), MemorySessionStore::default());

    if let Some(pwd) = &conf.labeler_password {
        agent.login(conf.handle.as_deref().unwrap_or(&conf.labeler_did), pwd).await?;
    } else {
        tracing::info!("Skipping batch due to missing password.");
        return Ok(());
    }

    let keypair = Arc::new(create_keypair(&conf.signing_key_hex)?);

    let rows = sqlx::query("SELECT DISTINCT uri FROM labels WHERE is_deleted = 0").fetch_all(&pool).await?;
    let local_dids: Vec<String> = rows.iter().map(|r| r.get("uri")).collect();
    tracing::info!(count = local_dids.len(), "Found local users");

    let mut followers_map = std::collections::HashMap::new();
    let mut cursor: Option<String> = None;
    let actor = conf.handle.as_deref().unwrap_or(&conf.labeler_did);

    loop {
        let resp = agent.api.app.bsky.graph.get_followers(
            atrium_api::app::bsky::graph::get_followers::ParametersData {
                actor: if actor.starts_with("did:") {
                    atrium_api::types::string::AtIdentifier::Did(atrium_api::types::string::Did::new(actor.to_string()).expect("Invalid DID"))
                } else {
                    atrium_api::types::string::AtIdentifier::Handle(atrium_api::types::string::Handle::new(actor.to_string()).expect("Invalid Handle"))
                }.into(),
                cursor: cursor.clone(),
                limit: Some((100 as u8).try_into().unwrap()),
            }.into()
        ).await?;

        for f in &resp.followers {
            followers_map.insert(f.did.as_str().to_string(), f.handle.as_str().to_string());
        }

        if resp.cursor.is_none() {
            break;
        }
        cursor = resp.cursor.clone();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    tracing::info!(count = followers_map.len(), "Fetched followers");

    for (did, handle) in &followers_map {
        if let Err(e) = assign_fortune(did, Some(handle), &pool, &keypair, &conf.labeler_did, &tx).await {
            tracing::error!(did, error = ?e, "Error assigning fortune");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    for did in local_dids {
        if !followers_map.contains_key(&did) {
            if let Err(e) = revoke_fortune(&did, &pool, &keypair, &config().labeler_did, &tx).await {
                tracing::error!(did, error = ?e, "Error revoking fortune");
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    tracing::info!("Batch complete");
    Ok(())
}

pub async fn run_migration(pool: DbPool, tx: broadcast::Sender<(i64, Vec<Label>)>) -> Result<()> {
    tracing::info!("Starting migration batch (ID Rotation)");
    let conf = config();

    let keypair = Arc::new(create_keypair(&conf.signing_key_hex)?);

    // 1. Get ALL users ever seen (even if soft deleted, we need to revoke their old ghosts)
    let rows = sqlx::query("SELECT DISTINCT uri FROM labels").fetch_all(&pool).await?;
    let all_dids: Vec<String> = rows.iter().map(|r| r.get("uri")).collect();
    tracing::info!(count = all_dids.len(), "Found ALL users for migration (active + inactive)");

    let _agent = AtpAgent::new(ReqwestClient::new("https://bsky.social"), MemorySessionStore::default());
    // Wait for at least one listener (AppView) to connect, otherwise events are lost in void.
    tracing::info!("Waiting for active listeners (AppView)...");
    let mut waits = 0;
    while tx.receiver_count() == 0 {
        if waits > 300 { // Wayyy too long (30s), assuming no one is coming.
             tracing::warn!("No listeners connected after 30s. Broadcasting anyway (might be lost).");
             break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        waits += 1;
    }
    tracing::info!(listeners = tx.receiver_count(), "Listeners active. Starting migration.");
    if let Some(_pwd) = &conf.labeler_password {
         // Login optional for migration but good for consistent object creation if needed?
         // assign_fortune doesn't use agent.
    }

    for did in all_dids {
        // Check if user is currently active (to decide whether to re-apply)
        let active_check = sqlx::query("SELECT 1 FROM labels WHERE uri = ? AND is_deleted = 0 LIMIT 1")
            .bind(&did)
            .fetch_optional(&pool)
            .await?;
        let is_active = active_check.is_some();
        // 2. Check for Fixed Status BEFORE deleting
        let current_labels = crate::db::get_labels(&pool, &did, None, None).await?;
        let fixed_entry = current_labels.iter().find(|l| l.is_fixed.unwrap_or(0) == 1 && l.neg == 0);

        let fixed_val = if let Some(f) = fixed_entry {
            // Map old string/new string to Enum.
            // Since fortune.rs was manually reverted to strict parsing, we must strip -new explicitly.
            let clean_val = f.val.replace("-new", "");
            Fortune::from_str(&clean_val).ok()
        } else {
            None
        };

        // 3. Force Revoke Old Labels (Blindly broadcast negation for all old strings)
        // This is necessary because previous migration might have soft-deleted them without broadcast,
        // leaving ghosts on the network but invisible to revoke_fortune checks.
        let old_label_strings = vec![
            "daikichi-new", "kichi-new", "chukichi-new", "shokichi-new", "suekichi-new", "kyo-new", "daikyo-new",
            "daikichi", "kichi", "chukichi", "shokichi", "suekichi", "kyo", "daikyo" // Also clean up original ghosts if any remain
        ];

        let mut force_negation_labels = Vec::new();
        let now_str = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let cts = atrium_api::types::string::Datetime::from_str(&now_str).expect("Invalid timestamp");

        for val in old_label_strings {
            let mut label_data = atrium_api::com::atproto::label::defs::LabelData {
                cid: None,
                cts: cts.clone(),
                exp: None,
                neg: Some(true),
                sig: None,
                src: atrium_api::types::string::Did::new(conf.labeler_did.to_string()).expect("Invalid DID"),
                uri: did.clone(),
                val: val.to_string(),
                ver: Some(1),
            };

            if let Ok(_) = crate::crypto::sign_label(&mut label_data, &keypair) {
                force_negation_labels.push(Label {
                    data: label_data,
                    extra_data: ipld_core::ipld::Ipld::Null,
                });
            }
        }

        if !force_negation_labels.is_empty() {
             // 0 sequence for revocation
             if let Err(_e) = tx.send((0, force_negation_labels)) {
                 tracing::error!(did, "Failed to broadcast force negations");
             } else {
                 tracing::info!(did, "Broadcasted FORCE negation for old labels");
             }
        }

        // Ensure DB is consistent (Soft Delete)
        if let Err(e) = crate::db::delete_label(&pool, &did).await {
             tracing::error!(did, error = ?e, "Error soft deleting label");
        }

        if is_active {
            // 4. Re-apply (Only for active users)
            if let Some(fortune_enum) = fixed_val {
                 // Was fixed. Re-apply using NEW ID string (as_str() returns new)
                 // overwrite_fortune sets is_fixed=true
                 if let Err(e) = overwrite_fortune(&did, fortune_enum.as_str(), &pool, &keypair, &conf.labeler_did, &tx).await {
                     tracing::error!(did, error = ?e, "Error migrating fixed fortune");
                 }
            } else {
                 // Was random. Re-roll (or re-calc deterministic)
                 if let Err(e) = assign_fortune(&did, None, &pool, &keypair, &conf.labeler_did, &tx).await {
                     tracing::error!(did, error = ?e, "Error migrating fortune");
                 }
            }
        } else {
            tracing::info!(did, "User is inactive, skipped re-application (Ghost cleanup only)");
        }

        // Throttle to avoid flooding broadcast channel too fast?
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    tracing::info!("Migration complete");
    Ok(())
}
