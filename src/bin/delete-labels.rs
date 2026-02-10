use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::agent::atp_agent::AtpAgent;
use atrium_xrpc_client::reqwest::ReqwestClient;
use omikuji::config::config;
use atrium_api::types::string::{Nsid, RecordKey};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config
    let conf = config();
    let did = &conf.labeler_did;
    let password = conf
        .labeler_password
        .as_ref()
        .expect("LABELER_PASSWORD must be set in .env");

    let agent = AtpAgent::new(
        ReqwestClient::new("https://bsky.social"),
        MemorySessionStore::default(),
    );

    println!("Logging in as {}", did);
    agent.login(did, password).await?;

    println!("Deleting label definitions...");

    let collection = Nsid::new("app.bsky.labeler.service".to_string()).unwrap();
    let rkey = RecordKey::new("self".to_string()).unwrap();

    let session = agent.get_session().await.expect("Not logged in");

    let input = atrium_api::com::atproto::repo::delete_record::InputData {
        collection,
        repo: session.did.clone().into(),
        rkey,
        swap_commit: None,
        swap_record: None,
    };

    agent.api.com.atproto.repo.delete_record(input.into()).await?;

    println!("Label definitions deleted!");

    Ok(())
}
