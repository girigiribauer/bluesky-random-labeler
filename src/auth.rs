use atrium_crypto::did::parse_multikey;
use atrium_crypto::verify::Verifier;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AuthError {
    #[error("missing bearer token")]
    Missing,
    #[error("malformed token")]
    Malformed,
    #[error("unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("token is for another service: {0}")]
    WrongAudience(String),
    #[error("token is for another method: {0}")]
    WrongMethod(String),
    #[error("token expired")]
    Expired,
    #[error("could not resolve signing key for {0}")]
    KeyUnavailable(String),
    #[error("bad signature")]
    BadSignature,
}

#[derive(Deserialize)]
struct TokenHeader {
    alg: String,
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    exp: i64,
    lxm: Option<String>,
}

#[derive(Clone)]
pub enum KeyResolver {
    Network,
    Fixed(Arc<HashMap<String, String>>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerificationMethod {
    id: String,
    public_key_multibase: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DidDocument {
    #[serde(default)]
    verification_method: Vec<VerificationMethod>,
}

fn did_document_url(did: &str) -> Option<String> {
    if did.starts_with("did:plc:") {
        return Some(format!("https://plc.directory/{}", did));
    }
    let host = did.strip_prefix("did:web:")?;
    if host.is_empty() || host.contains(':') || host.contains('/') {
        return None;
    }
    Some(format!("https://{}/.well-known/did.json", host))
}

fn atproto_key(doc: &DidDocument) -> Option<String> {
    doc.verification_method
        .iter()
        .find(|m| m.id.ends_with("#atproto"))
        .and_then(|m| m.public_key_multibase.clone())
}

impl KeyResolver {
    pub async fn atproto_key(&self, did: &str) -> Result<String, AuthError> {
        let unavailable = || AuthError::KeyUnavailable(did.to_string());
        match self {
            KeyResolver::Fixed(keys) => keys.get(did).cloned().ok_or_else(unavailable),
            KeyResolver::Network => {
                let url = did_document_url(did).ok_or_else(unavailable)?;
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|_| unavailable())?;
                let doc: DidDocument = client
                    .get(url)
                    .send()
                    .await
                    .and_then(|r| r.error_for_status())
                    .map_err(|_| unavailable())?
                    .json()
                    .await
                    .map_err(|_| unavailable())?;
                atproto_key(&doc).ok_or_else(unavailable)
            }
        }
    }
}

fn decode_part<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, AuthError> {
    let bytes = URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).map_err(|_| AuthError::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| AuthError::Malformed)
}

pub async fn verify_service_auth(
    authorization: Option<&str>,
    audience: &str,
    method: &str,
    resolver: &KeyResolver,
    now: i64,
) -> Result<String, AuthError> {
    let token = authorization
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or(AuthError::Missing)?;

    let mut parts = token.split('.');
    let (Some(header_part), Some(claims_part), Some(sig_part), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(AuthError::Malformed);
    };

    let header: TokenHeader = decode_part(header_part)?;
    if header.alg != "ES256K" && header.alg != "ES256" {
        return Err(AuthError::UnsupportedAlgorithm(header.alg));
    }

    let claims: Claims = decode_part(claims_part)?;
    let labeler_audience = format!("{}#atproto_labeler", audience);
    if claims.aud != audience && claims.aud != labeler_audience {
        return Err(AuthError::WrongAudience(claims.aud));
    }
    if let Some(lxm) = &claims.lxm {
        if lxm != method {
            return Err(AuthError::WrongMethod(lxm.clone()));
        }
    }
    if claims.exp <= now {
        return Err(AuthError::Expired);
    }

    let issuer = claims.iss.split('#').next().unwrap_or_default().to_string();
    let multikey = resolver.atproto_key(&issuer).await?;
    let (alg, public_key) = parse_multikey(&multikey).map_err(|_| AuthError::KeyUnavailable(issuer.clone()))?;
    let signature = URL_SAFE_NO_PAD.decode(sig_part.trim_end_matches('=')).map_err(|_| AuthError::Malformed)?;
    let signing_input = format!("{}.{}", header_part, claims_part);
    Verifier::default()
        .verify(alg, &public_key, signing_input.as_bytes(), &signature)
        .map_err(|_| AuthError::BadSignature)?;

    Ok(issuer)
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use atrium_crypto::keypair::{Did, Secp256k1Keypair};

    pub fn multikey(keypair: &Secp256k1Keypair) -> String {
        keypair.did().trim_start_matches("did:key:").to_string()
    }

    pub fn resolver_for(entries: &[(&str, &Secp256k1Keypair)]) -> KeyResolver {
        let keys = entries.iter().map(|(did, kp)| (did.to_string(), multikey(kp))).collect();
        KeyResolver::Fixed(Arc::new(keys))
    }

    pub fn make_token(keypair: &Secp256k1Keypair, alg: &str, claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(serde_json::json!({ "typ": "JWT", "alg": alg }).to_string());
        let body = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing_input = format!("{}.{}", header, body);
        let sig = URL_SAFE_NO_PAD.encode(keypair.sign(signing_input.as_bytes()).unwrap());
        format!("Bearer {}.{}", signing_input, sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::*;
    use atrium_crypto::keypair::Secp256k1Keypair;

    const LABELER: &str = "did:plc:labeler";
    const USER: &str = "did:plc:user";
    const METHOD: &str = "com.atproto.moderation.createReport";
    const NOW: i64 = 1_800_000_000;

    fn keypair() -> Secp256k1Keypair {
        Secp256k1Keypair::create(&mut rand::rngs::OsRng)
    }

    fn claims() -> serde_json::Value {
        serde_json::json!({ "iss": USER, "aud": LABELER, "exp": NOW + 60, "lxm": METHOD })
    }

    async fn verify(token: &str, resolver: &KeyResolver) -> Result<String, AuthError> {
        verify_service_auth(Some(token), LABELER, METHOD, resolver, NOW).await
    }

    /// 送り主の鍵で正しく署名された証明なら、送り主の DID を返すこと
    #[tokio::test]
    async fn test_valid_token_returns_issuer() {
        let kp = keypair();
        let token = make_token(&kp, "ES256K", claims());

        assert_eq!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Ok(USER.to_string()));
    }

    /// 宛先に #atproto_labeler が付いた証明も受け付けること
    #[tokio::test]
    async fn test_audience_with_labeler_fragment_is_accepted() {
        let kp = keypair();
        let mut c = claims();
        c["aud"] = serde_json::json!(format!("{}#atproto_labeler", LABELER));
        let token = make_token(&kp, "ES256K", c);

        assert_eq!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Ok(USER.to_string()));
    }

    /// 証明が付いていないときは断ること
    #[tokio::test]
    async fn test_missing_token_is_rejected() {
        let result = verify_service_auth(None, LABELER, METHOD, &resolver_for(&[]), NOW).await;

        assert_eq!(result, Err(AuthError::Missing));
    }

    /// 形の壊れた証明は断ること
    #[tokio::test]
    async fn test_malformed_token_is_rejected() {
        assert_eq!(verify("Bearer not-a-token", &resolver_for(&[])).await, Err(AuthError::Malformed));
    }

    /// 別のラベラー宛ての証明は断ること
    #[tokio::test]
    async fn test_other_audience_is_rejected() {
        let kp = keypair();
        let mut c = claims();
        c["aud"] = serde_json::json!("did:plc:someone-else");
        let token = make_token(&kp, "ES256K", c);

        assert!(matches!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Err(AuthError::WrongAudience(_))));
    }

    /// 通報以外の操作向けの証明は断ること
    #[tokio::test]
    async fn test_other_method_is_rejected() {
        let kp = keypair();
        let mut c = claims();
        c["lxm"] = serde_json::json!("com.atproto.label.queryLabels");
        let token = make_token(&kp, "ES256K", c);

        assert!(matches!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Err(AuthError::WrongMethod(_))));
    }

    /// 期限の切れた証明は断ること
    #[tokio::test]
    async fn test_expired_token_is_rejected() {
        let kp = keypair();
        let mut c = claims();
        c["exp"] = serde_json::json!(NOW);
        let token = make_token(&kp, "ES256K", c);

        assert_eq!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Err(AuthError::Expired));
    }

    /// 送り主になりすまして別の鍵で署名した証明は断ること
    #[tokio::test]
    async fn test_token_signed_by_another_key_is_rejected() {
        let real = keypair();
        let attacker = keypair();
        let token = make_token(&attacker, "ES256K", claims());

        assert_eq!(verify(&token, &resolver_for(&[(USER, &real)])).await, Err(AuthError::BadSignature));
    }

    /// 署名したあとで中身を書き換えた証明は断ること
    #[tokio::test]
    async fn test_tampered_claims_are_rejected() {
        let kp = keypair();
        let token = make_token(&kp, "ES256K", claims());
        let mut parts: Vec<String> = token.trim_start_matches("Bearer ").split('.').map(String::from).collect();
        let mut c = claims();
        c["iss"] = serde_json::json!("did:plc:victim");
        parts[1] = URL_SAFE_NO_PAD.encode(c.to_string());
        let tampered = format!("Bearer {}", parts.join("."));

        let resolver = resolver_for(&[(USER, &kp), ("did:plc:victim", &kp)]);
        assert_eq!(verify(&tampered, &resolver).await, Err(AuthError::BadSignature));
    }

    /// 送り主の鍵が見つからないときは断ること
    #[tokio::test]
    async fn test_unknown_issuer_is_rejected() {
        let kp = keypair();
        let token = make_token(&kp, "ES256K", claims());

        assert!(matches!(verify(&token, &resolver_for(&[])).await, Err(AuthError::KeyUnavailable(_))));
    }

    /// 決まりにない署名方式の証明は断ること
    #[tokio::test]
    async fn test_unsupported_algorithm_is_rejected() {
        let kp = keypair();
        let token = make_token(&kp, "HS256", claims());

        assert!(matches!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Err(AuthError::UnsupportedAlgorithm(_))));
    }

    /// P-256 の鍵で署名された証明も受け付けること
    #[tokio::test]
    async fn test_p256_token_is_accepted() {
        use atrium_crypto::keypair::{Did, P256Keypair};
        let kp = P256Keypair::create(&mut rand::rngs::OsRng);
        let header = URL_SAFE_NO_PAD.encode(serde_json::json!({ "typ": "JWT", "alg": "ES256" }).to_string());
        let body = URL_SAFE_NO_PAD.encode(claims().to_string());
        let signing_input = format!("{}.{}", header, body);
        let sig = URL_SAFE_NO_PAD.encode(kp.sign(signing_input.as_bytes()).unwrap());
        let token = format!("Bearer {}.{}", signing_input, sig);
        let keys = [(USER.to_string(), kp.did().trim_start_matches("did:key:").to_string())].into_iter().collect();

        assert_eq!(verify(&token, &KeyResolver::Fixed(Arc::new(keys))).await, Ok(USER.to_string()));
    }

    /// 操作の指定（lxm）がない証明も受け付けること
    #[tokio::test]
    async fn test_token_without_method_is_accepted() {
        let kp = keypair();
        let mut c = claims();
        c.as_object_mut().unwrap().remove("lxm");
        let token = make_token(&kp, "ES256K", c);

        assert_eq!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Ok(USER.to_string()));
    }

    /// 送り主に「#」以降が付いていても、DID の部分を送り主として返すこと
    #[tokio::test]
    async fn test_issuer_fragment_is_stripped() {
        let kp = keypair();
        let mut c = claims();
        c["iss"] = serde_json::json!(format!("{}#atproto", USER));
        let token = make_token(&kp, "ES256K", c);

        assert_eq!(verify(&token, &resolver_for(&[(USER, &kp)])).await, Ok(USER.to_string()));
    }

    /// DID 文書から、#atproto の公開鍵を取り出すこと
    #[test]
    fn test_atproto_key_is_read_from_did_document() {
        let doc: DidDocument = serde_json::from_value(serde_json::json!({
            "id": "did:plc:abc",
            "verificationMethod": [
                { "id": "did:plc:abc#other", "type": "Multikey", "controller": "did:plc:abc", "publicKeyMultibase": "zOther" },
                { "id": "did:plc:abc#atproto", "type": "Multikey", "controller": "did:plc:abc", "publicKeyMultibase": "zQ3shXRoj5DKMsoaCXQauLrmGzJdMxXYYuous32qsViEm8fwP" }
            ]
        })).unwrap();

        assert_eq!(atproto_key(&doc), Some("zQ3shXRoj5DKMsoaCXQauLrmGzJdMxXYYuous32qsViEm8fwP".to_string()));
    }

    /// DID 文書に #atproto の公開鍵がなければ、見つからない扱いにすること
    #[test]
    fn test_atproto_key_is_missing() {
        let doc: DidDocument = serde_json::from_value(serde_json::json!({ "id": "did:plc:abc" })).unwrap();

        assert_eq!(atproto_key(&doc), None);
    }

    /// DID の種類に応じて、DID 文書を読みに行く先を決めること
    #[test]
    fn test_did_document_url() {
        assert_eq!(did_document_url("did:plc:abc"), Some("https://plc.directory/did:plc:abc".to_string()));
        assert_eq!(did_document_url("did:web:example.com"), Some("https://example.com/.well-known/did.json".to_string()));
        assert_eq!(did_document_url("did:web:example.com:8080"), None);
        assert_eq!(did_document_url("did:key:z123"), None);
    }
}
