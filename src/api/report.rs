use axum::{Json, extract::State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use atrium_api::com::atproto::moderation::create_report::{Input, OutputSubjectRefs, OutputData};
use chrono::FixedOffset;
use crate::api::appeal::{apply_self_appeal, authenticate, did_of_uri};
use crate::state::AppState;
use atrium_api::types::string::{Did, Datetime};
use atrium_api::com::atproto::repo::strong_ref::MainData;
use ipld_core::ipld::Ipld;
use cid::Cid;
use std::str::FromStr;
use tracing;

const CREATE_REPORT_METHOD: &str = "com.atproto.moderation.createReport";

pub async fn create_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Response {
use atrium_api::com::atproto::moderation::create_report::InputSubjectRefs;
use atrium_api::types::Union;

    let reporter = match authenticate(&headers, CREATE_REPORT_METHOD, &state).await {
        Ok(did) => did,
        Err(response) => return response,
    };

    let did = match &input.subject {
        Union::Refs(InputSubjectRefs::ComAtprotoAdminDefsRepoRef(r)) => Some(r.did.as_str()),
        Union::Refs(InputSubjectRefs::ComAtprotoRepoStrongRefMain(r)) => did_of_uri(&r.uri),
        _ => None,
    };
    apply_self_appeal(&state, &reporter, did, input.reason.as_deref()).await;

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
    let reported_by = Did::new(reporter).unwrap_or_else(|_| Did::new("did:plc:unknown".to_string()).unwrap());
    Json::<atrium_api::com::atproto::moderation::create_report::Output>(OutputData {
        created_at: Datetime::new(now),
        id: 12345, // Dummy ID
        reason: input.reason.clone(),
        reason_type: input.reason_type.clone(),
        reported_by,
        subject: Union::Refs(subject),
    }.into()).into_response()
}
