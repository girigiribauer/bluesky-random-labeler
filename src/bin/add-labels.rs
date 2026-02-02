use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::agent::atp_agent::AtpAgent;
use atrium_api::app::bsky::labeler::defs::LabelerPoliciesData;
use atrium_api::app::bsky::labeler::service::RecordData;
use atrium_api::com::atproto::label::defs::{
    LabelValueDefinitionData, LabelValueDefinitionStringsData,
    LabelValueDefinition,
};
use atrium_api::types::string::{Datetime, Language, Nsid, RecordKey};
use atrium_api::types::Unknown;
use atrium_xrpc_client::reqwest::ReqwestClient;
use omikuji::config::config;
use std::str::FromStr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config
    let conf = config();
    let did = &conf.labeler_did;
    let password = conf
        .labeler_password
        .as_ref()
        .expect("LABELER_PASSWORD must be set in .env");

    println!("Logged in as {}", did);
    println!("Adding label definitions...");

    let agent = AtpAgent::new(
        ReqwestClient::new("https://bsky.social"),
        MemorySessionStore::default(),
    );

    agent.login(did, password).await?;

    let labels = vec![
        ("daikichi", "大吉", "今日の運勢は大吉！最高の一日があなたを待ってる！"),
        ("kichi", "吉", "今日の運勢は吉！楽しい一日になりそう！"),
        ("chukichi", "中吉", "今日の運勢は中吉！楽しんでいこ！"),
        ("shokichi", "小吉", "今日の運勢は小吉！小さな幸せ見つけよう！"),
        ("suekichi", "末吉", "今日の運勢は末吉！すえひろがりな一日を！"),
        ("kyo", "凶", "今日の運勢は凶。気を引き締めていこう！"),
        ("daikyo", "大凶", "今日の運勢は大凶。無理せず慎重に！"),
    ];

    let label_values: Vec<String> = labels.iter().map(|(id, _, _)| id.to_string()).collect();

    let label_value_definitions: Vec<LabelValueDefinition> = labels
        .into_iter()
        .map(|(id, name, desc)| {
            LabelValueDefinitionData {
                identifier: id.to_string(),
                severity: "inform".to_string(),
                blurs: "none".to_string(),
                default_setting: Some("warn".to_string()),
                locales: vec![LabelValueDefinitionStringsData {
                    lang: Language::from_str("ja").unwrap(),
                    name: name.to_string(),
                    description: desc.to_string(),
                }.into()],
                adult_only: None,
            }
            .into()
        })
        .collect();

    let record_data = RecordData {
        created_at: Datetime::now(),
        labels: None,
        policies: LabelerPoliciesData {
            label_values,
            label_value_definitions: Some(label_value_definitions),
        }
        .into(),
        reason_types: None,
        subject_collections: None,
        subject_types: None,
    };

    println!("Sending putRecord request...");

    let collection = Nsid::new("app.bsky.labeler.service".to_string()).unwrap();
    let rkey = RecordKey::new("self".to_string()).unwrap();

    let session = agent.get_session().await.expect("Not logged in");

    // Convert to serde_json::Value then Deserialize to Unknown
    let record_value = serde_json::to_value(record_data)?;
    let unknown_record: Unknown = serde_json::from_value(record_value)?;

    let input = atrium_api::com::atproto::repo::put_record::InputData {
        collection,
        repo: session.did.clone().into(),
        rkey,
        record: unknown_record,
        swap_commit: None,
        swap_record: None,
        validate: Some(true),
    };

    let result = agent.api.com.atproto.repo.put_record(input.into()).await?;

    println!("Label definitions added! URI: {}, CID: {:?}", result.uri, result.cid);

    Ok(())
}
