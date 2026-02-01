use axum::{Json, extract::State};
use crate::api::QsQuery;
use atrium_api::com::atproto::label::query_labels::{Parameters, Output, OutputData};
use atrium_api::com::atproto::label::defs::Label;
use crate::db::get_labels;
use crate::config::config;
use crate::crypto::sign_label;
use atrium_api::types::string::{Did, Datetime};
use chrono::SubsecRound;
use crate::state::AppState;
use tracing;

pub async fn query_labels(
    State(state): State<AppState>,
    QsQuery(params): QsQuery<Parameters>,
) -> Json<Output> {
    let cursor = params.cursor.clone().and_then(|c| c.parse::<i64>().ok());

    tracing::debug!(?params.data.uri_patterns, ?params.cursor, ?params.limit, "REQ queryLabels");

    let input = params.data;
    let mut labels = Vec::new();
    let labeler_did = &config().labeler_did;
    let mut last_id = 0;

    for pattern in input.uri_patterns {
        let rows = get_labels(&state.pool, &pattern, cursor, input.limit.map(|l| u8::from(l).into())).await.unwrap_or_else(|_| vec![]);

        for row in rows {
            if row.id > last_id {
                last_id = row.id;
            }
            let uri = row.uri;
            let val = row.val;
            let cts_str = row.cts;
            let neg_int = row.neg;
            let src = row.src;

            let cts_parsed = chrono::DateTime::parse_from_rfc3339(&cts_str)
                .unwrap_or_else(|_| chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())).round_subsecs(3);

            let mut label_data = atrium_api::com::atproto::label::defs::LabelData {
                cid: None,
                cts: Datetime::new(cts_parsed),
                exp: None,
                neg: if neg_int != 0 { Some(true) } else { None },
                sig: None,
                src: Did::new(src).unwrap_or_else(|_| Did::new(labeler_did.clone()).unwrap()),
                uri,
                val,
                ver: None,
            };

            if let Err(e) = sign_label(&mut label_data, &state.keypair) {
                tracing::error!(error = ?e, "Failed to sign label");
                continue;
            }

            labels.push(Label::from(label_data));
        }
    }

    let next_cursor = if last_id > 0 { Some(last_id.to_string()) } else { None };

    tracing::debug!(count = labels.len(), ?next_cursor, "RES queryLabels");

    Json(OutputData {
        cursor: next_cursor,
        labels,
    }.into())
}
