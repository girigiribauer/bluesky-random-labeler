use axum::{Json, extract::State};
use atrium_api::com::atproto::moderation::create_report::{Input, Output, OutputSubjectRefs, OutputData};
use chrono::FixedOffset;
use crate::state::AppState;
use crate::config::config;
use crate::domain::fortune::FORTUNES;
use crate::domain::labeling::overwrite_fortune;
use atrium_api::types::string::{Did, Datetime};
use atrium_api::com::atproto::repo::strong_ref::MainData;
use ipld_core::ipld::Ipld;
use cid::Cid;
use std::str::FromStr;
use tracing;

pub async fn create_report(
    State(state): State<AppState>,
    Json(input): Json<Input>,
) -> Json<Output> {
use atrium_api::com::atproto::moderation::create_report::InputSubjectRefs;
use atrium_api::types::Union;

    if let Some(reason) = &input.reason {
        let mut best_match: Option<&str> = None;
        let mut best_len = 0;

        for f in FORTUNES {
            if reason.contains(f.val.as_str()) {
                if f.val.as_str().len() > best_len {
                    best_match = Some(f.val.as_str());
                    best_len = f.val.as_str().len();
                }
            }
            if reason.contains(f.label) {
                if f.label.len() > best_len {
                    best_match = Some(f.val.as_str());
                    best_len = f.label.len();
                }
            }
        }

        if let Some(val) = best_match {
            let did = match &input.subject {
                Union::Refs(InputSubjectRefs::ComAtprotoAdminDefsRepoRef(r)) => Some(r.did.as_str()),
                Union::Refs(InputSubjectRefs::ComAtprotoRepoStrongRefMain(r)) => match atrium_api::types::string::Did::new(r.uri.clone()) {
                     Ok(_) => Some(r.uri.as_str()),
                     _ => if r.uri.starts_with("at://") {
                         r.uri.split('/').nth(2)
                     } else {
                         None
                     }
                },
                _ => None,
            };

            if let Some(did_str) = did {
                tracing::info!(val, did = did_str, "Gimmick Triggered! Forcing fortune");
                if let Err(e) = overwrite_fortune(
                    did_str,
                    val,
                    &state.pool,
                    &state.keypair,
                    &config().labeler_did,
                    &state.tx
                ).await {
                    tracing::error!(error = ?e, "Failed to overwrite fortune");
                } else {
                    tracing::info!("Fortune overwritten successfully");
                }
            } else {
                tracing::warn!("Gimmick: Failed to extract DID from subject");
            }
        } else {
            tracing::debug!(reason, "Gimmick: No matching fortune keyword found");
        }
    } else {
        tracing::debug!("Gimmick: No reason provided in report");
    }

    let subject = match &input.subject {
        Union::Refs(InputSubjectRefs::ComAtprotoAdminDefsRepoRef(r)) => OutputSubjectRefs::ComAtprotoAdminDefsRepoRef(r.clone()),
        Union::Refs(InputSubjectRefs::ComAtprotoRepoStrongRefMain(r)) => OutputSubjectRefs::ComAtprotoRepoStrongRefMain(r.clone()),
        _ => {
            tracing::warn!("Unknown subject type received");
            // Fallback to a dummy strongRef to avoid crashing
            OutputSubjectRefs::ComAtprotoRepoStrongRefMain(Box::new(atrium_api::com::atproto::repo::strong_ref::Main {
                data: MainData {
                    cid: atrium_api::types::string::Cid::new(Cid::from_str("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").expect("Invalid dummy CID")),
                    uri: "at://did:plc:dummy/app.bsky.feed.post/3juv3456789".to_string(),
                },
                extra_data: Ipld::Null,
            }))
        },
    };

    let now = chrono::Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    Json(OutputData {
        created_at: Datetime::new(now),
        id: 12345, // Dummy ID
        reason: input.reason.clone(),
        reason_type: input.reason_type.clone(),
        reported_by: Did::new("did:plc:unknown".to_string()).unwrap(),
        subject: Union::Refs(subject),
    }.into())
}
