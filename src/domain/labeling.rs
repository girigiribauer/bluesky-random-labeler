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
    keypair: &Secp256k1Keypair,
    labeler_did: &str,
    tx: &broadcast::Sender<(i64, Vec<Label>)>
) -> Result<()> {
    // 1. Fetch current active labels
    let active_labels = db_get_labels(pool, did, None, None).await?;
    let mut negation_labels = Vec::new();

    let now_str = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let cts = Datetime::from_str(&now_str).expect("Invalid timestamp");

    for l in active_labels {
        // Only negate positive labels to avoid redundancy
        if l.neg != 0 { continue; }

        let mut label_data = LabelData {
            cid: None,
            cts: cts.clone(),
            exp: None,
            neg: Some(true),
            sig: None,
            src: Did::new(labeler_did.to_string()).expect("Invalid DID"),
            uri: l.uri.clone(),
            val: l.val.clone(),
            ver: Some(1),
        };

        sign_label(&mut label_data, keypair)?;

        negation_labels.push(Label {
            data: label_data,
            extra_data: ipld_core::ipld::Ipld::Null,
        });
    }

    // 2. Broadcast negation labels
    if !negation_labels.is_empty() {
        // Use 0 as sequence number for revocation events as we don't produce new DB rows
        match tx.send((0, negation_labels)) {
            Ok(count) => tracing::info!(did, count, "Broadcasted negation labels"),
            Err(_) => tracing::debug!(did, "No listeners active for negation broadcast"),
        }
    }

    // 3. Soft Delete from DB
    db_delete(pool, did).await?;
    tracing::info!(did, "Revoked fortune (Soft Delete complete)");

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

    /// おみくじを引くと、当たった1つのラベルと、ほかの6つを打ち消すラベルが保存されること
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

    const LABELER: &str = "did:plc:labeler";
    const TARGET: &str = "did:plc:target";

    async fn setup() -> Result<(DbPool, Secp256k1Keypair, broadcast::Sender<(i64, Vec<Label>)>)> {
        let pool = init_db(":memory:").await?;
        let keypair = Secp256k1Keypair::create(&mut rand::rngs::OsRng);
        let (tx, _rx) = broadcast::channel(100);
        Ok((pool, keypair, tx))
    }

    fn other_than_daily(did: &str) -> Fortune {
        let daily = get_daily_fortune(did);
        FORTUNES.iter().map(|f| f.val).find(|&v| v != daily).unwrap()
    }

    /// 書き換えると、指定したおみくじが当たりになり、ほかの6つは打ち消しになること
    #[tokio::test]
    async fn test_overwrite_fortune_sets_given_fortune() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;

        overwrite_fortune(TARGET, "daikyo", &pool, &keypair, LABELER, &tx).await?;

        let labels = get_labels(&pool, TARGET, None, None).await?;
        let positives: Vec<_> = labels.iter().filter(|l| l.neg == 0).map(|l| l.val.as_str()).collect();
        assert_eq!(positives, vec!["daikyo"]);
        assert_eq!(labels.iter().filter(|l| l.neg == 1).count(), 6);

        Ok(())
    }

    /// 書き換えたおみくじには、固定の印が付くこと
    #[tokio::test]
    async fn test_overwrite_fortune_marks_fixed() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;

        overwrite_fortune(TARGET, "daikyo", &pool, &keypair, LABELER, &tx).await?;

        let labels = get_labels(&pool, TARGET, None, None).await?;
        let positive = labels.iter().find(|l| l.neg == 0).unwrap();
        assert_eq!(positive.is_fixed.unwrap_or(0), 1);

        Ok(())
    }

    /// おみくじにない値で書き換えようとすると、エラーになり何も保存されないこと
    #[tokio::test]
    async fn test_overwrite_fortune_rejects_unknown_value() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;

        let result = overwrite_fortune(TARGET, "chokichi", &pool, &keypair, LABELER, &tx).await;

        assert!(result.is_err());
        assert!(get_labels(&pool, TARGET, None, None).await?.is_empty());

        Ok(())
    }

    /// その日に書き換えたおみくじは、引き直しても上書きされないこと
    #[tokio::test]
    async fn test_assign_fortune_keeps_todays_overwrite() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;
        let fixed = other_than_daily(TARGET);

        overwrite_fortune(TARGET, fixed.as_str(), &pool, &keypair, LABELER, &tx).await?;
        assign_fortune(TARGET, None, &pool, &keypair, LABELER, &tx).await?;

        let labels = get_labels(&pool, TARGET, None, None).await?;
        let positives: Vec<_> = labels.iter().filter(|l| l.neg == 0).map(|l| l.val.as_str()).collect();
        assert_eq!(positives, vec![fixed.as_str()]);

        Ok(())
    }

    /// 取り消した人のラベルは、読み出されなくなること
    #[tokio::test]
    async fn test_revoke_fortune_hides_labels() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;

        assign_fortune(TARGET, None, &pool, &keypair, LABELER, &tx).await?;
        revoke_fortune(TARGET, &pool, &keypair, LABELER, &tx).await?;

        assert!(get_labels(&pool, TARGET, None, None).await?.is_empty());

        Ok(())
    }

    /// 取り消すと、当たりだったラベルを打ち消すラベルが配信されること
    #[tokio::test]
    async fn test_revoke_fortune_broadcasts_negation() -> Result<()> {
        let (pool, keypair, tx) = setup().await?;

        assign_fortune(TARGET, None, &pool, &keypair, LABELER, &tx).await?;
        let mut rx = tx.subscribe();
        revoke_fortune(TARGET, &pool, &keypair, LABELER, &tx).await?;

        let (_, labels) = rx.recv().await?;
        let negated: Vec<_> = labels.iter().map(|l| (l.data.val.as_str(), l.data.neg)).collect();
        assert_eq!(negated, vec![(get_daily_fortune(TARGET).as_str(), Some(true))]);

        Ok(())
    }
}
