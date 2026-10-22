//! Giving an action's secrets to the worker that runs it, and masking
//! them in what it prints.
//!
//! An action that needs secrets names them only; its digest, its stored
//! `Command` and its cache key never hold a value. When a worker takes it,
//! the scheduler writes a copy of the command with the values, sealed, in
//! the space's records, and sends the worker an action pointing at that
//! copy. Only workers can read it (`worker_read`), and it is deleted when
//! the operation finishes.

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::command::EnvironmentVariable;
use rill_reapi_proto::reapi::{Action, Command, ExecuteResponse};
use rill_types::ObjectHash;

use super::redact::Redactor;
use super::store::command_namespace;
use crate::error::{Error, Result};
use crate::remote::operation::OperationRecord;
use crate::remote::{BlobDigest, ExecSpace, Scheduler};

impl Scheduler {
    /// `action_bytes` with its command replaced by one carrying `op`'s
    /// secrets, and the digest of that command.
    pub(crate) async fn with_secrets(
        &self,
        space: &ExecSpace,
        op: &OperationRecord,
        action_bytes: Bytes,
    ) -> Result<(Bytes, ObjectHash)> {
        let mut action = Action::decode(action_bytes)
            .map_err(|e| Error::Codec(format!("action {}: {e}", op.action)))?;
        let digest = BlobDigest::parse(
            action
                .command_digest
                .as_ref()
                .ok_or_else(|| Error::InvalidAction("no command digest".into()))?,
        )?;
        let bytes = space
            .read(&digest)
            .await?
            .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
        let mut command =
            Command::decode(bytes).map_err(|e| Error::Codec(format!("command {digest}: {e}")))?;
        let secrets = self.open_secrets(&space.org, &op.secrets).await?;
        command
            .environment_variables
            .retain(|v| !secrets.iter().any(|(name, _)| *name == v.name));
        for (name, value) in secrets {
            command.environment_variables.push(EnvironmentVariable {
                name,
                value: String::from_utf8(value)
                    .map_err(|_| Error::InvalidAction("a secret value is not UTF-8".into()))?,
            });
        }
        command
            .environment_variables
            .sort_by(|a, b| a.name.cmp(&b.name));
        let bytes = command.encode_to_vec();
        let with_values = BlobDigest::of(&bytes);
        let sealed = self
            .seal(&command_namespace(&space.org), &with_values.hash, &bytes)
            .await?;
        space
            .records
            .meta()
            .set(
                space.records.keys().exec_secret_command(&with_values.hash),
                Bytes::from(sealed),
            )
            .await?;
        action.command_digest = Some(with_values.to_proto());
        Ok((Bytes::from(action.encode_to_vec()), with_values.hash))
    }

    /// Content `digest` names, for a worker: the space's content, or a
    /// command carrying secrets for an action it runs.
    pub async fn worker_read(
        &self,
        space: &ExecSpace,
        digest: &BlobDigest,
    ) -> Result<Option<Bytes>> {
        if let Some(bytes) = space.read(digest).await? {
            return Ok(Some(bytes));
        }
        let key = space.records.keys().exec_secret_command(&digest.hash);
        let Some(sealed) = space.records.meta().get(&key).await? else {
            return Ok(None);
        };
        let bytes = Bytes::from(
            self.open(
                &command_namespace(&space.org),
                &digest.hash,
                sealed.to_vec(),
            )
            .await?,
        );
        digest.verify(&bytes)?;
        Ok(Some(bytes))
    }

    /// Mask `op`'s secret values in the output `response` points at,
    /// storing masked copies in their place.
    pub(crate) async fn redact_response(
        &self,
        space: &ExecSpace,
        op: &OperationRecord,
        response: &mut ExecuteResponse,
    ) -> Result<()> {
        let Some(result) = response.result.as_mut() else {
            return Ok(());
        };
        let secrets = self.open_secrets(&space.org, &op.secrets).await?;
        let Some(redactor) = Redactor::new(&secrets) else {
            return Ok(());
        };
        redact_stream(
            space,
            &redactor,
            &mut result.stdout_digest,
            &mut result.stdout_raw,
        )
        .await?;
        redact_stream(
            space,
            &redactor,
            &mut result.stderr_digest,
            &mut result.stderr_raw,
        )
        .await?;
        Ok(())
    }
}

async fn redact_stream(
    space: &ExecSpace,
    redactor: &Redactor,
    digest: &mut Option<rill_reapi_proto::reapi::Digest>,
    raw: &mut Bytes,
) -> Result<()> {
    if let Some(masked) = redactor.redact(raw) {
        *raw = Bytes::from(masked);
    }
    let Some(stored) = digest.as_ref().map(BlobDigest::parse).transpose()? else {
        return Ok(());
    };
    let Some(bytes) = space.read(&stored).await? else {
        return Ok(());
    };
    if let Some(masked) = redactor.redact(&bytes) {
        *digest = Some(space.put(Bytes::from(masked)).await?.to_proto());
    }
    Ok(())
}
