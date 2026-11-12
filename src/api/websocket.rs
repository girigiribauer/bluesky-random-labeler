use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query, State},
    http::HeaderMap,
    response::Response,
};
use atrium_api::com::atproto::label::subscribe_labels::{Labels, LabelsData};
use atrium_api::com::atproto::label::defs::{Label, LabelData};
use atrium_api::types::string::{Datetime, Did};
use chrono::SubsecRound;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::MissedTickBehavior;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use crate::api::AppState;
use crate::crypto::sign_label;
use crate::db::{labels_after, LabelRow};
use ipld_core::ipld::Ipld;

const REPLAY_PAGE_SIZE: i64 = 100;
const RECENT_SEND_WINDOW: Duration = Duration::from_secs(120);
const RECONNECT_WATCH_WINDOW: Duration = Duration::from_secs(300);

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);
static LAST_ABNORMAL_CLOSE: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    ClientClose { code: Option<u16>, reason: String },
    RecvError(String),
    Eof,
    SendFailed,
    Lagged(u64),
    ChannelClosed,
}

pub fn is_abnormal_close(reason: &CloseReason, since_last_send: Option<Duration>) -> bool {
    match reason {
        CloseReason::SendFailed | CloseReason::Lagged(_) => true,
        _ => since_last_send.is_some_and(|d| d < RECENT_SEND_WINDOW),
    }
}

struct ConnInfo {
    id: u64,
    peer: String,
    user_agent: String,
    cursor: Option<i64>,
    connected_at: Instant,
}

#[derive(Serialize, Deserialize, Debug)]
struct StreamHeader {
    t: String,
    op: i64,
}

#[derive(Deserialize, Debug)]
pub struct SubscribeLabelsParams {
    pub cursor: Option<i64>,
}

pub async fn subscribe_labels(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<SubscribeLabelsParams>,
    headers: HeaderMap,
) -> Response {
    let header = |name: &str| {
        headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("-").to_string()
    };
    let info = ConnInfo {
        id: NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed),
        peer: header("x-forwarded-for"),
        user_agent: header("user-agent"),
        cursor: params.cursor,
        connected_at: Instant::now(),
    };
    tracing::debug!(conn = info.id, peer = %info.peer, cursor = ?info.cursor, "WS: subscribeLabels request received");
    ws.on_upgrade(move |socket| handle_socket(socket, state, info))
}

fn encode_frame(seq: i64, labels: Vec<Label>) -> Option<Vec<u8>> {
    // Construct Header
    let header = StreamHeader {
        t: "#labels".to_string(),
        op: 1, // Frame
    };

    // Construct Body
    let body = Labels {
        data: LabelsData {
            seq,
            labels,
        },
        extra_data: Ipld::Null,
    };

    // Serialize
    match (serde_ipld_dagcbor::to_vec(&header), serde_ipld_dagcbor::to_vec(&body)) {
        (Ok(header_bytes), Ok(body_bytes)) => {
            // Combine [Header][Body]
            let mut payload = header_bytes;
            payload.extend(body_bytes);
            Some(payload)
        }
        _ => {
            tracing::error!("Failed to serialize label update");
            None
        }
    }
}

fn row_to_label(row: &LabelRow, state: &AppState) -> Option<Label> {
    let cts = match chrono::DateTime::parse_from_rfc3339(&row.cts) {
        Ok(cts) => cts.round_subsecs(3),
        Err(e) => {
            tracing::warn!(seq = row.id, cts = %row.cts, error = ?e, "WS: Skipped replay row with invalid cts");
            return None;
        }
    };
    let src = match Did::new(row.src.clone()) {
        Ok(src) => src,
        Err(e) => {
            tracing::warn!(seq = row.id, src = %row.src, error = ?e, "WS: Skipped replay row with invalid src");
            return None;
        }
    };
    let mut label_data = LabelData {
        cid: None,
        cts: Datetime::new(cts),
        exp: None,
        neg: if row.neg != 0 { Some(true) } else { None },
        sig: None,
        src,
        uri: row.uri.clone(),
        val: row.val.clone(),
        ver: Some(1),
    };
    if let Err(e) = sign_label(&mut label_data, &state.keypair) {
        tracing::error!(error = ?e, "Failed to sign label");
        return None;
    }
    Some(Label { data: label_data, extra_data: Ipld::Null })
}

async fn send_frame(socket: &mut WebSocket, seq: i64, labels: Vec<Label>) -> Result<(), ()> {
    let Some(payload) = encode_frame(seq, labels) else {
        return Ok(());
    };
    // Send
    if let Err(e) = socket.send(Message::Binary(payload)).await {
        tracing::warn!(error = ?e, "WS: Failed to send message");
        return Err(());
    }
    tracing::debug!(seq, "WS: Sent message to client");
    Ok(())
}

fn recent_abnormal_close() -> bool {
    LAST_ABNORMAL_CLOSE
        .lock()
        .ok()
        .and_then(|last| *last)
        .is_some_and(|t| t.elapsed() < RECONNECT_WATCH_WINDOW)
}

fn mark_abnormal_close() {
    if let Ok(mut last) = LAST_ABNORMAL_CLOSE.lock() {
        *last = Some(Instant::now());
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState, info: ConnInfo) {
    tracing::debug!(conn = info.id, "WS: Connection established");
    if recent_abnormal_close() {
        tracing::info!(conn = info.id, peer = %info.peer, user_agent = %info.user_agent, cursor = ?info.cursor, "WS: Reconnected after abnormal close");
    }
    let mut rx = state.tx.subscribe();

    let mut replay_after = info.cursor;
    let mut pending: VecDeque<LabelRow> = VecDeque::new();
    let mut tick = tokio::time::interval(state.replay_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut replayed_through: i64 = 0;
    let mut last_seq: i64 = 0;
    let mut sent: u64 = 0;
    let mut replayed: u64 = 0;
    let mut last_sent_at: Option<Instant> = None;
    let close_reason;

    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(frame))) => {
                    close_reason = CloseReason::ClientClose {
                        code: frame.as_ref().map(|f| f.code),
                        reason: frame.map(|f| f.reason.to_string()).unwrap_or_default(),
                    };
                    break;
                }
                Some(Err(e)) => {
                    close_reason = CloseReason::RecvError(e.to_string());
                    break;
                }
                None => {
                    close_reason = CloseReason::Eof;
                    break;
                }
                Some(Ok(_)) => {}
            },
            _ = tick.tick(), if replay_after.is_some() => {
                if pending.is_empty() {
                    let after = replay_after.unwrap_or(0);
                    match labels_after(&state.pool, after, REPLAY_PAGE_SIZE).await {
                        Ok(rows) if !rows.is_empty() => {
                            if replayed == 0 {
                                tracing::info!(conn = info.id, peer = %info.peer, cursor = ?info.cursor, "WS: Replay started");
                            }
                            pending.extend(rows);
                        }
                        Ok(_) => {
                            if replayed > 0 {
                                tracing::info!(conn = info.id, replayed, last_seq, "WS: Replay finished");
                            }
                            replay_after = None;
                            continue;
                        }
                        Err(e) => {
                            tracing::error!(conn = info.id, error = ?e, "WS: Replay query failed");
                            replay_after = None;
                            continue;
                        }
                    }
                }
                if let Some(row) = pending.pop_front() {
                    replay_after = Some(row.id);
                    let Some(label) = row_to_label(&row, &state) else { continue };
                    if send_frame(&mut socket, row.id, vec![label]).await.is_err() {
                        close_reason = CloseReason::SendFailed;
                        break;
                    }
                    replayed_through = row.id;
                    last_seq = row.id;
                    last_sent_at = Some(Instant::now());
                    sent += 1;
                    replayed += 1;
                }
            },
            broadcasted = rx.recv() => match broadcasted {
                Ok((seq, labels)) => {
                    tracing::debug!(seq, count = labels.len(), "WS: Received broadcast");
                    if seq != 0 && (replay_after.is_some() || seq <= replayed_through) {
                        continue;
                    }
                    if send_frame(&mut socket, seq, labels).await.is_err() {
                        close_reason = CloseReason::SendFailed;
                        break;
                    }
                    if seq > last_seq {
                        last_seq = seq;
                    }
                    last_sent_at = Some(Instant::now());
                    sent += 1;
                }
                Err(RecvError::Lagged(n)) => {
                    close_reason = CloseReason::Lagged(n);
                    break;
                }
                Err(RecvError::Closed) => {
                    close_reason = CloseReason::ChannelClosed;
                    break;
                }
            },
        }
    }

    let since_last_send = last_sent_at.map(|t| t.elapsed());
    let duration_secs = info.connected_at.elapsed().as_secs();
    if is_abnormal_close(&close_reason, since_last_send) {
        mark_abnormal_close();
        tracing::warn!(
            conn = info.id,
            peer = %info.peer,
            user_agent = %info.user_agent,
            cursor = ?info.cursor,
            duration_secs,
            since_last_send_ms = since_last_send.map(|d| d.as_millis() as u64),
            sent,
            replayed,
            last_seq,
            reason = ?close_reason,
            "WS: Connection closed while labels may be in flight"
        );
    } else {
        tracing::debug!(conn = info.id, duration_secs, sent, last_seq, reason = ?close_reason, "WS: Connection closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 何も送っていない接続が閉じられても、異常として扱わないこと
    #[test]
    fn test_close_without_any_send_is_normal() {
        let reason = CloseReason::ClientClose { code: Some(1000), reason: String::new() };
        assert!(!is_abnormal_close(&reason, None));
    }

    /// 送ってから2分以内に閉じられたら、閉じ方にかかわらず異常として扱うこと
    #[test]
    fn test_close_soon_after_send_is_abnormal() {
        let soon = Some(Duration::from_secs(30));
        assert!(is_abnormal_close(&CloseReason::ClientClose { code: Some(1000), reason: String::new() }, soon));
        assert!(is_abnormal_close(&CloseReason::Eof, soon));
        assert!(is_abnormal_close(&CloseReason::RecvError("reset".to_string()), soon));
    }

    /// 最後に送ってから2分以上たってから閉じられたなら、異常として扱わないこと
    #[test]
    fn test_close_long_after_send_is_normal() {
        let long_ago = Some(Duration::from_secs(120));
        assert!(!is_abnormal_close(&CloseReason::ClientClose { code: Some(1000), reason: String::new() }, long_ago));
        assert!(!is_abnormal_close(&CloseReason::Eof, long_ago));
    }

    /// こちらの送信に失敗したときは、最後に送った時期にかかわらず異常として扱うこと
    #[test]
    fn test_send_failure_is_abnormal() {
        assert!(is_abnormal_close(&CloseReason::SendFailed, None));
        assert!(is_abnormal_close(&CloseReason::SendFailed, Some(Duration::from_secs(600))));
    }

    /// 配信の受け渡しがあふれたときは、異常として扱うこと
    #[test]
    fn test_lagged_is_abnormal() {
        assert!(is_abnormal_close(&CloseReason::Lagged(5), None));
    }
}
