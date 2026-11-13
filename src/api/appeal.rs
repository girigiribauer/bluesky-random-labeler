use axum::{Json, extract::State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use crate::auth::verify_service_auth;
use crate::config::config;
use crate::db::get_labels;
use crate::domain::fortune::FORTUNES;
use crate::domain::labeling::overwrite_fortune;
use crate::state::AppState;

pub const APPEAL_METHOD: &str = "tools.ozone.inbox.appealActionedSubject";

#[derive(Debug, PartialEq)]
pub enum AppealOutcome {
    Applied(&'static str),
    NoFortuneWord,
    NoSubject,
    NotSelf,
    Failed,
}

pub async fn authenticate(headers: &HeaderMap, method: &str, state: &AppState) -> Result<String, Response> {
    let authorization = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    verify_service_auth(
        authorization,
        &config().labeler_did,
        method,
        &state.key_resolver,
        chrono::Utc::now().timestamp(),
    )
    .await
    .map_err(|e| {
        tracing::warn!(method, error = %e, "Appeal rejected: authentication failed");
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "AuthenticationRequired", "message": e.to_string() })),
        )
            .into_response()
    })
}

pub fn fortune_in(reason: &str) -> Option<&'static str> {
    let mut best_match: Option<&'static str> = None;
    let mut best_len = 0;

    for f in FORTUNES {
        if reason.contains(f.val.as_str()) && f.val.as_str().len() > best_len {
            best_match = Some(f.val.as_str());
            best_len = f.val.as_str().len();
        }
        if reason.contains(f.label) && f.label.len() > best_len {
            best_match = Some(f.val.as_str());
            best_len = f.label.len();
        }
    }
    best_match
}

pub fn did_of_uri(uri: &str) -> Option<&str> {
    if uri.starts_with("did:") {
        return Some(uri);
    }
    uri.strip_prefix("at://")?.split('/').next().filter(|d| d.starts_with("did:"))
}

fn subject_did(subject: &Value) -> Option<&str> {
    match subject.get("$type").and_then(Value::as_str) {
        Some("com.atproto.admin.defs#repoRef") => subject.get("did").and_then(Value::as_str),
        Some("com.atproto.repo.strongRef") => subject.get("uri").and_then(Value::as_str).and_then(did_of_uri),
        _ => None,
    }
}

pub async fn apply_self_appeal(state: &AppState, reporter: &str, subject: Option<&str>, reason: Option<&str>) -> AppealOutcome {
    let Some(val) = reason.and_then(fortune_in) else {
        tracing::debug!(?reason, "Gimmick: No matching fortune keyword found");
        return AppealOutcome::NoFortuneWord;
    };
    let Some(did) = subject else {
        tracing::warn!("Gimmick: Failed to extract DID from subject");
        return AppealOutcome::NoSubject;
    };
    if did != reporter {
        tracing::info!(val, did, reporter, "Gimmick: Ignored appeal for someone else");
        return AppealOutcome::NotSelf;
    }

    tracing::info!(val, did, "Gimmick Triggered! Forcing fortune");
    match overwrite_fortune(did, val, &state.pool, &state.keypair, &config().labeler_did, &state.tx).await {
        Ok(()) => {
            tracing::info!("Fortune overwritten successfully");
            AppealOutcome::Applied(val)
        }
        Err(e) => {
            tracing::error!(error = ?e, "Failed to overwrite fortune");
            AppealOutcome::Failed
        }
    }
}

#[derive(Deserialize)]
pub struct AppealInput {
    pub subject: Value,
    pub reason: Option<String>,
}

fn invalid_subject() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "InvalidAppealSubject", "message": "Only your own account can be appealed" })),
    )
        .into_response()
}

pub async fn appeal_actioned_subject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AppealInput>,
) -> Response {
    let reporter = match authenticate(&headers, APPEAL_METHOD, &state).await {
        Ok(did) => did,
        Err(response) => return response,
    };

    let Some(subject) = subject_did(&input.subject) else {
        tracing::warn!("Appeal rejected: subject is not an account or a post");
        return invalid_subject();
    };
    if subject != reporter {
        tracing::info!(subject, reporter = %reporter, "Appeal rejected: subject is someone else");
        return invalid_subject();
    }
    let outcome = apply_self_appeal(&state, &reporter, Some(subject), input.reason.as_deref()).await;

    let labels: Vec<String> = get_labels(&state.pool, &reporter, None, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|l| l.neg == 0)
        .map(|l| l.val)
        .collect();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let decision = if matches!(outcome, AppealOutcome::Applied(_)) { "approved" } else { "denied" };

    Json(json!({
        "src": config().labeler_did,
        "subject": input.subject,
        "enforcement": { "state": "labeled", "labels": labels },
        "appeal": { "state": "resolved", "appealedAt": now, "resolvedAt": now, "outcome": decision },
        "isRead": true,
        "createdAt": now,
        "updatedAt": now,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 理由の文章に重なって含まれる運勢の言葉は、長いほう（「吉」より「大吉」）を取り出すこと
    #[test]
    fn test_fortune_in_prefers_longer_word() {
        assert_eq!(fortune_in("大吉にして"), Some("daikichi"));
    }

    /// 理由の文章にローマ字の運勢の値（daikyo など）があれば、それを取り出すこと
    #[test]
    fn test_fortune_in_reads_romaji_value() {
        assert_eq!(fortune_in("daikyo please"), Some("daikyo"));
    }

    /// 理由の文章に運勢の言葉がなければ、何も取り出さないこと
    #[test]
    fn test_fortune_in_without_word() {
        assert_eq!(fortune_in("ただの文章"), None);
    }

    /// アカウント指定の対象から、そのアカウントの DID を取り出すこと
    #[test]
    fn test_subject_did_from_repo_ref() {
        let subject = json!({ "$type": "com.atproto.admin.defs#repoRef", "did": "did:plc:a" });
        assert_eq!(subject_did(&subject), Some("did:plc:a"));
    }

    /// 投稿指定の対象から、投稿の持ち主の DID を取り出すこと
    #[test]
    fn test_subject_did_from_strong_ref() {
        let subject = json!({ "$type": "com.atproto.repo.strongRef", "uri": "at://did:plc:b/app.bsky.feed.post/1", "cid": "x" });
        assert_eq!(subject_did(&subject), Some("did:plc:b"));
    }

    /// 決まりにない書き方の対象からは、DID を取り出さないこと
    #[test]
    fn test_subject_did_from_unknown_type() {
        let subject = json!({ "$type": "unknown", "did": "did:plc:c" });
        assert_eq!(subject_did(&subject), None);
    }
}
