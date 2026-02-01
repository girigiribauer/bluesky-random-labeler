use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::Response,
};
use atrium_api::com::atproto::label::subscribe_labels::{Labels, LabelsData};
use atrium_api::com::atproto::label::defs::Label;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use crate::api::AppState;
use ipld_core::ipld::Ipld;

#[derive(Serialize, Deserialize, Debug)]
struct StreamHeader {
    t: String,
    op: i64,
}

pub async fn subscribe_labels(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    tracing::info!("WS: subscribeLabels request received");
    ws.on_upgrade(move |socket| handle_socket(socket, state.tx))
}

async fn handle_socket(mut socket: WebSocket, tx: broadcast::Sender<(i64, Vec<Label>)>) {
    tracing::info!("WS: Connection established");
    let mut rx = tx.subscribe();

    loop {
        match rx.recv().await {
            Ok((seq, labels)) => {
                tracing::debug!(seq, count = labels.len(), "WS: Received broadcast");
                // Construct Header
                let header = StreamHeader {
                    t: "#labels".to_string(),
                    op: 1, // Frame
                };

                // Construct Body
                let body = Labels {
                    data: LabelsData {
                        seq,
                        labels: labels.clone(),
                    },
                    extra_data: Ipld::Null,
                };

                // Serialize
                match (serde_ipld_dagcbor::to_vec(&header), serde_ipld_dagcbor::to_vec(&body)) {
                    (Ok(header_bytes), Ok(body_bytes)) => {
                        // Combine [Header][Body]
                        let mut payload = header_bytes;
                        payload.extend(body_bytes);

                        // Send
                        if let Err(e) = socket.send(Message::Binary(payload)).await {
                            tracing::warn!(error = ?e, "WS: Failed to send message");
                            break;
                        } else {
                            tracing::debug!("WS: Sent message to client");
                        }
                    }
                    _ => {
                         tracing::error!("Failed to serialize label update");
                    }
                }
            }
            Err(e) => {
                tracing::debug!(error = ?e, "WS: Broadcast channel closed or lagged");
                break;
            }
        }
    }
    tracing::info!("WS: Connection closed");
}
