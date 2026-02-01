use atrium_api::com::atproto::label::defs::LabelData;
use atrium_crypto::keypair::Secp256k1Keypair;
use anyhow::Result;

pub fn sign_label(label: &mut LabelData, keypair: &Secp256k1Keypair) -> Result<()> {
    label.sig = None;

    let bytes = serde_ipld_dagcbor::to_vec(label)?;
    let signature = keypair.sign(&bytes)?;

    label.sig = Some(signature);

    Ok(())
}

pub fn create_keypair(hex: &str) -> Result<Secp256k1Keypair> {
    let bytes = hex::decode(hex)?;
    Secp256k1Keypair::import(&bytes).map_err(|e| anyhow::anyhow!("Invalid key: {}", e))
}
