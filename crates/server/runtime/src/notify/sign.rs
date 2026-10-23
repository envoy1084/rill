//! Standard Webhooks signatures (<https://www.standardwebhooks.com>): an
//! HMAC-SHA256 over `{id}.{timestamp}.{body}`, sent as `v1,<base64>`.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// The headers a signed delivery carries.
pub const ID_HEADER: &str = "webhook-id";
pub const TIMESTAMP_HEADER: &str = "webhook-timestamp";
pub const SIGNATURE_HEADER: &str = "webhook-signature";

/// The signing key a secret names: a `whsec_`-prefixed secret is the
/// base64 of the key, anything else is the key itself.
pub fn key(secret: &[u8]) -> Vec<u8> {
    match secret.strip_prefix(b"whsec_") {
        Some(b64) => base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap_or_else(|_| secret.to_vec()),
        None => secret.to_vec(),
    }
}

/// The `webhook-signature` value for a delivery.
pub fn signature(secret: &[u8], id: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key(secret)).expect("HMAC takes a key of any length");
    mac.update(id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let tag = mac.finalize().into_bytes();
    format!(
        "v1,{}",
        base64::engine::general_purpose::STANDARD.encode(tag)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_standard_webhooks_test_vector() {
        let sig = signature(
            b"whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
            "msg_p5jXN8AQM9LWM0D4loKWxJek",
            1614265330,
            br#"{"test": 2432232314}"#,
        );
        assert_eq!(sig, "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=");
    }
}
