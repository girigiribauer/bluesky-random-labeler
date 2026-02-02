use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub type DbPool = Pool<Sqlite>;

pub async fn init_db(db_path: &str) -> Result<DbPool> {
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let db_url = format!("sqlite:{}?mode=rwc", db_path);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS labels (
          uri TEXT NOT NULL,
          val TEXT NOT NULL,
          cts TEXT NOT NULL,
          neg INTEGER DEFAULT 0,
          src TEXT,
          is_fixed INTEGER DEFAULT 0,
          is_deleted INTEGER DEFAULT 0,
          PRIMARY KEY (uri, val)
        );
        "#
    )
    .execute(&pool)
    .await?;

    // カラム追加は2回目以降エラーになるが、エラーを無視して続ける（重複していたら追加されない）
    let _ = sqlx::query("ALTER TABLE labels ADD COLUMN is_fixed INTEGER DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE labels ADD COLUMN is_deleted INTEGER DEFAULT 0")
        .execute(&pool)
        .await;

    Ok(pool)
}

pub async fn upsert_label(pool: &DbPool, uri: &str, val: &str, cts: &str, neg: bool, src: &str, is_fixed: bool) -> Result<i64> {
    // Manually delete duplicates to ensure uniqueness on legacy schemas without explicit PK
    sqlx::query("DELETE FROM labels WHERE uri = ? AND val = ?")
        .bind(uri)
        .bind(val)
        .execute(pool)
        .await?;

    let neg_int = if neg { 1 } else { 0 };
    let fixed_int = if is_fixed { 1 } else { 0 };
    let result = sqlx::query("INSERT INTO labels (uri, val, cts, neg, src, is_fixed) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(uri)
        .bind(val)
        .bind(cts)
        .bind(neg_int)
        .bind(src)
        .bind(fixed_int)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_label(pool: &DbPool, uri: &str) -> Result<()> {
    // Soft delete: Update is_deleted flag and update timestamp
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query("UPDATE labels SET is_deleted = 1, cts = ? WHERE uri = ?")
        .bind(now)
        .bind(uri)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct LabelRow {
    pub id: i64,
    pub uri: String,
    pub val: String,
    pub cts: String,
    pub neg: i32,
    pub src: String,
    pub is_fixed: Option<i32>,
    pub is_deleted: Option<i32>,
}

pub async fn get_labels(pool: &DbPool, uri: &str, cursor: Option<i64>, limit: Option<i64>) -> Result<Vec<LabelRow>> {
    let limit = limit.unwrap_or(50);
    let cursor = cursor.unwrap_or(0);

    let rows = sqlx::query_as::<_, LabelRow>(
        "SELECT rowid as id, uri, val, cts, neg, src, is_fixed, is_deleted FROM labels WHERE uri = ? AND rowid > ? AND is_deleted = 0 ORDER BY rowid DESC LIMIT ?"
    )
        .bind(uri)
        .bind(cursor)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_operations() -> Result<()> {
        let pool = init_db(":memory:").await?;
        let uri = "did:plc:test";
        let val = "daikichi";
        let cts = "2026-01-01T00:00:00Z";
        let src = "did:plc:issuer";

        upsert_label(&pool, uri, val, cts, false, src, false).await?;

        let labels = get_labels(&pool, uri, None, None).await?;
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].uri, uri);
        assert_eq!(labels[0].val, val);
        assert_eq!(labels[0].neg, 0);
        assert_eq!(labels[0].is_fixed.unwrap_or(0), 0);

        let new_val = "chukichi";
        upsert_label(&pool, uri, new_val, cts, false, src, true).await?;

        let labels_updated = get_labels(&pool, uri, None, None).await?;
        assert_eq!(labels_updated.len(), 2);
        assert_eq!(labels_updated[0].is_fixed.unwrap_or(0), 1);

        let neg_uri = "did:plc:negated";
        upsert_label(&pool, neg_uri, "kyo", cts, true, src, false).await?;
        let items = get_labels(&pool, neg_uri, None, None).await?;
        assert_eq!(items[0].neg, 1);

        delete_label(&pool, uri).await?;
        let empty = get_labels(&pool, uri, None, None).await?;
        assert_eq!(empty.len(), 0);

        Ok(())
    }
}
