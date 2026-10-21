//! AWS KMS as a key service.

use async_trait::async_trait;
use aws_sdk_kms::primitives::Blob;
use rill_types::identity::WrappedSecret;

use super::wrapper::KeyWrapper;
use crate::error::{Error, Result};

const BACKEND: &str = "aws-kms";

/// Wraps secrets with a symmetric KMS key through `Encrypt`/`Decrypt`.
pub struct AwsKms {
    client: aws_sdk_kms::Client,
    key_id: String,
}

impl AwsKms {
    /// A client from the default AWS configuration (environment, profile,
    /// instance or workload identity), optionally in `region` and against
    /// `endpoint` (LocalStack in tests).
    pub async fn connect(key_id: &str, region: Option<&str>, endpoint: Option<&str>) -> Self {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = region {
            loader = loader.region(aws_config::Region::new(region.to_string()));
        }
        if let Some(endpoint) = endpoint {
            loader = loader.endpoint_url(endpoint);
        }
        let config = loader.load().await;
        AwsKms {
            client: aws_sdk_kms::Client::new(&config),
            key_id: key_id.to_string(),
        }
    }
}

#[async_trait]
impl KeyWrapper for AwsKms {
    fn backend(&self) -> &'static str {
        BACKEND
    }

    fn key_ref(&self) -> &str {
        &self.key_id
    }

    async fn wrap(&self, plaintext: &[u8]) -> Result<WrappedSecret> {
        let out = self
            .client
            .encrypt()
            .key_id(&self.key_id)
            .plaintext(Blob::new(plaintext))
            .encryption_context("purpose", "rill-token-key")
            .send()
            .await
            .map_err(|e| Error::key_service(BACKEND, aws_sdk_kms::error::DisplayErrorContext(e)))?;
        let blob = out
            .ciphertext_blob()
            .ok_or_else(|| Error::key_service(BACKEND, "encrypt returned no ciphertext"))?;
        Ok(WrappedSecret {
            backend: BACKEND.into(),
            key_ref: self.key_id.clone(),
            ciphertext: blob.as_ref().to_vec(),
        })
    }

    async fn unwrap(&self, wrapped: &WrappedSecret) -> Result<Vec<u8>> {
        if !self.can_unwrap(wrapped) {
            return Err(Error::key_service(
                BACKEND,
                format!(
                    "secret was wrapped by {}/{}",
                    wrapped.backend, wrapped.key_ref
                ),
            ));
        }
        let out = self
            .client
            .decrypt()
            .key_id(&self.key_id)
            .ciphertext_blob(Blob::new(wrapped.ciphertext.clone()))
            .encryption_context("purpose", "rill-token-key")
            .send()
            .await
            .map_err(|e| Error::key_service(BACKEND, aws_sdk_kms::error::DisplayErrorContext(e)))?;
        out.plaintext()
            .map(|b| b.as_ref().to_vec())
            .ok_or_else(|| Error::key_service(BACKEND, "decrypt returned no plaintext"))
    }
}
