use crate::db::DbPool;
use atrium_api::com::atproto::label::defs::Label;
use std::sync::Arc;
use atrium_crypto::keypair::Secp256k1Keypair;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub keypair: Arc<Secp256k1Keypair>,
    pub tx: tokio::sync::broadcast::Sender<(i64, Vec<Label>)>,
}
