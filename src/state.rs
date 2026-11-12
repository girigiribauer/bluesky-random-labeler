use crate::db::DbPool;
use atrium_api::com::atproto::label::defs::Label;
use std::sync::Arc;
use atrium_crypto::keypair::Secp256k1Keypair;
use std::time::Duration;
use crate::auth::KeyResolver;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub keypair: Arc<Secp256k1Keypair>,
    pub tx: tokio::sync::broadcast::Sender<(i64, Vec<Label>)>,
    pub replay_interval: Duration,
    pub key_resolver: KeyResolver,
}
