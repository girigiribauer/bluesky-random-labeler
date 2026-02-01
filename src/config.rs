use std::env;
use std::sync::OnceLock;
use dotenvy::dotenv;

#[derive(Debug)]
pub struct Config {
    pub port: u16,
    pub db_path: String,
    pub labeler_did: String,
    pub signing_key_hex: String, // Hex encoded private key
    pub labeler_password: Option<String>, // For generic bot login if needed? Or actually handling handle/password
    pub handle: Option<String>,
}

pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        dotenv().ok();

        Config {
            port: env::var("PORT").unwrap_or_else(|_| "3000".to_string()).parse().expect("PORT must be a number"),
            db_path: env::var("DB_PATH").unwrap_or_else(|_| "data/labels.db".to_string()),
            labeler_did: env::var("LABELER_DID").expect("LABELER_DID must be set"),
            signing_key_hex: env::var("SIGNING_KEY").expect("SIGNING_KEY must be set"),
            labeler_password: env::var("LABELER_PASSWORD").ok(),
            handle: env::var("HANDLE").ok(), // Use this to authenticate for polling?
        }
    })
}
