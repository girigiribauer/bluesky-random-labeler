use crate::db::{DbPool, upsert_label as db_upsert, delete_label as db_delete, get_labels as db_get_labels};
use crate::domain::fortune::{get_daily_fortune, FORTUNES, Fortune};
use std::str::FromStr;
use crate::crypto::sign_label;
use atrium_crypto::keypair::Secp256k1Keypair;
use atrium_api::com::atproto::label::defs::{Label, LabelData};
use atrium_api::types::string::{Datetime, Did};
use chrono::Utc;
use tracing;
use anyhow::Result;
use tokio::sync::broadcast;

pub async fn assign_fortune(
    did: &str,
    handle: Option<&str>,
    pool: &DbPool,
    keypair: &Secp256k1Keypair,
    labeler_did: &str,
    tx: &broadcast::Sender<(i64, Vec<Label>)>
) -> Result<()> {
    let current_labels = db_get_labels(pool, did, None, None).await?;
    let _now_str = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true); // Same format as in upsert
    if let Some(fixed_label) = current_labels.iter().find(|l| l.is_fixed.unwrap_or(0) == 1 && l.neg == 0) {
        if let Ok(fixed_date) = chrono::DateTime::parse_from_rfc3339(&fixed_label.cts) {
            let now = Utc::now();
            let trunc_fixed = fixed_date.with_timezone(&chrono::FixedOffset::east_opt(9*3600).unwrap()).date_naive();
            let trunc_now = now.with_timezone(&chrono::FixedOffset::east_opt(9*3600).unwrap()).date_naive();

            if trunc_fixed == trunc_now {
                 tracing::info!(did, "Skipping assignment due to manual override (is_fixed=true)");
                 return Ok(());
            }
        }
    }

    let fortune = get_daily_fortune(did);
    let handle_str = handle.unwrap_or("unknown");
    tracing::info!(did, handle = %handle_str, %fortune, "Processing user");

    let negate_list: Vec<Fortune> = FORTUNES.iter()
        .map(|f| f.val)
        .filter(|&v| v != fortune)
        .collect();

    upsert_label(did, fortune.as_str(), false, labeler_did, pool, keypair, tx, false).await?;

    for neg_fortune in negate_list {
        upsert_label(did, neg_fortune.as_str(), true, labeler_did, pool, keypair, tx, false).await?;
    }

    Ok(())
}

pub async fn overwrite_fortune(
    did: &str,
    fortune_val: &str,
    pool: &DbPool,
    keypair: &Secp256k1Keypair,
    labeler_did: &str,
    tx: &broadcast::Sender<(i64, Vec<Label>)>
) -> Result<()> {
    // Validate fortune_val
    let fortune = match Fortune::from_str(fortune_val) {
        Ok(f) => f,
        Err(_) => return Err(anyhow::anyhow!("Invalid fortune value: {}", fortune_val)),
    };

    let negate_list: Vec<Fortune> = FORTUNES.iter()
        .map(|f| f.val)
        .filter(|&v| v != fortune)
        .collect();

    upsert_label(did, fortune.as_str(), false, labeler_did, pool, keypair, tx, true).await?;
    for neg_fortune in negate_list {
        upsert_label(did, neg_fortune.as_str(), true, labeler_did, pool, keypair, tx, true).await?;
    }
    Ok(())
}

pub async fn revoke_fortune(
    did: &str,
    pool: &DbPool,
    _keypair: &Secp256k1Keypair,
    _labeler_did: &str,
    _tx: &broadcast::Sender<(i64, Vec<Label>)>
) -> Result<()> {
    db_delete(pool, did).await?;
    Ok(())
}

async fn upsert_label(
    uri: &str,
    val: &str,
    neg: bool,
    src: &str,
    pool: &DbPool,
    keypair: &Secp256k1Keypair,
    tx: &broadcast::Sender<(i64, Vec<Label>)>,
    is_fixed: bool
) -> Result<()> {
use std::str::FromStr;
    let now_str = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let cts = Datetime::from_str(&now_str).expect("Invalid timestamp");

    let mut label_data = LabelData {
        cid: None,
        cts: cts.clone(),
        exp: None,
        neg: if neg { Some(true) } else { None },
        sig: None,
        src: Did::new(src.to_string()).expect("Invalid DID"), // Ensure config DID is valid
        uri: uri.to_string(),
        val: val.to_string(),
        ver: Some(1),
    };

    sign_label(&mut label_data, keypair)?;

    let rowid = db_upsert(pool, uri, val, &cts.as_ref().to_rfc3339(), neg, src, is_fixed).await?;

    // Create Label struct for broadcast
    let label = Label {
        data: label_data.clone(),
        extra_data: ipld_core::ipld::Ipld::Null,
    };

    // Broadcast
    match tx.send((rowid, vec![label])) {
        Ok(count) => tracing::debug!(listeners = count, seq = rowid, val, "Broadcaster sent label"),
        Err(_) => tracing::debug!(seq = rowid, val, "Broadcaster: No listeners active"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_db, get_labels};

    #[tokio::test]
    async fn test_assign_fortune_logic() -> Result<()> {
        let pool = init_db(":memory:").await?;
        use rand::rngs::OsRng;
        let mut rng = OsRng;
        let keypair = Secp256k1Keypair::create(&mut rng);
        let labeler_did = "did:plc:labeler";
        let target_did = "did:plc:target";
        let (tx, _rx) = broadcast::channel(100);

        assign_fortune(target_did, None, &pool, &keypair, labeler_did, &tx).await?;

        let labels = get_labels(&pool, target_did, None, None).await?;
        assert!(!labels.is_empty(), "Labels should be created");

        let positives: Vec<_> = labels.iter().filter(|l| l.neg == 0).collect();
        let negatives: Vec<_> = labels.iter().filter(|l| l.neg == 1).collect();

        assert_eq!(positives.len(), 1, "Should have exactly 1 positive label");
        assert_eq!(negatives.len(), 6, "Should have 6 negative labels");
        assert_eq!(labels.len(), 7, "Total labels should be 7");

        println!("Granted fortune: {}", positives[0].val);

        Ok(())
    }
}
