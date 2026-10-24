//! Derived values and named per-repository values.
//!
//! Both can grow large (search postings, import maps, landing rings), so a
//! value above [`OFFLOAD_BYTES`] is stored as a `Derived` object and the
//! metadata row keeps only its hash. Rows start with a marker byte: `0`
//! for the value inline, `1` for a 32-byte object hash.

use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind};

use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::object::ObjectStoreExt;

/// Values larger than this live in the object store.
pub const OFFLOAD_BYTES: usize = 64 * 1024;

const INLINE: u8 = 0;
const OBJECT: u8 = 1;

impl RepoStore {
    pub async fn get_derived(&self, kind: &str, hash: &ObjectHash) -> Result<Option<Bytes>> {
        let row = self.meta().get(&self.keys().derived(kind, hash)).await?;
        self.open_row(row).await
    }

    /// Several derived values of one kind, in one round trip.
    pub async fn get_derived_many(
        &self,
        kind: &str,
        hashes: &[ObjectHash],
    ) -> Result<Vec<Option<Bytes>>> {
        let keys: Vec<Bytes> = hashes
            .iter()
            .map(|h| self.keys().derived(kind, h))
            .collect();
        let rows = self.meta().get_many(&keys).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.open_row(row).await?);
        }
        Ok(out)
    }

    /// Store several derived values of one kind in one transaction, which
    /// fails once `fence` (when given) is lost.
    pub async fn put_derived_many(
        &self,
        kind: &str,
        values: &[(ObjectHash, Bytes)],
        fence: Option<&super::leases::Lease>,
    ) -> Result<()> {
        let mut tx = crate::meta::Transaction::new();
        for (hash, value) in values {
            tx.mutations
                .push(self.derived_mutation(kind, hash, value.clone()).await?);
        }
        if let Some(lease) = fence {
            tx.conditions.push(lease.fence(self));
        }
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// The write storing a derived value, for a larger transaction.
    pub async fn derived_mutation(
        &self,
        kind: &str,
        hash: &ObjectHash,
        value: Bytes,
    ) -> Result<crate::meta::Mutation> {
        Ok(crate::meta::Mutation::Set {
            key: self.keys().derived(kind, hash),
            value: self.seal_row(value).await?,
        })
    }

    pub async fn put_derived(&self, kind: &str, hash: &ObjectHash, value: Bytes) -> Result<()> {
        let row = self.seal_row(value).await?;
        self.meta().set(self.keys().derived(kind, hash), row).await
    }

    /// Remove a derived value; an offloaded object stays for the
    /// collector.
    pub async fn delete_derived(&self, kind: &str, hash: &ObjectHash) -> Result<()> {
        self.meta()
            .transact(crate::meta::Transaction::new().delete(self.keys().derived(kind, hash)))
            .await?;
        Ok(())
    }

    /// [`RepoStore::put_derived`] that fails once `fence` is lost.
    pub async fn put_derived_fenced(
        &self,
        kind: &str,
        hash: &ObjectHash,
        value: Bytes,
        fence: &super::leases::Lease,
    ) -> Result<()> {
        let row = self.seal_row(value).await?;
        let mut tx = crate::meta::Transaction::new().set(self.keys().derived(kind, hash), row);
        tx.conditions.push(fence.fence(self));
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn get_misc(&self, name: &str) -> Result<Option<Bytes>> {
        let row = self.meta().get(&self.keys().misc(name)).await?;
        self.open_row(row).await
    }

    pub async fn set_misc(&self, name: &str, value: Bytes) -> Result<()> {
        let row = self.seal_row(value).await?;
        self.meta().set(self.keys().misc(name), row).await
    }

    /// [`RepoStore::set_misc`] together with the event it causes.
    pub async fn set_misc_with_event(
        &self,
        name: &str,
        value: Bytes,
        event: &rill_types::Event,
    ) -> Result<()> {
        let row = self.seal_row(value).await?;
        let mut tx = crate::meta::Transaction::new().set(self.keys().misc(name), row);
        tx.mutations.extend(self.event(event)?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// [`RepoStore::set_misc`] that writes only while the value is still
    /// `expected` (`None`: absent); a conflict error otherwise.
    pub async fn set_misc_cas(
        &self,
        name: &str,
        expected: Option<Bytes>,
        value: Bytes,
    ) -> Result<()> {
        let expected = match expected {
            Some(old) => Some(self.seal_row(old).await?),
            None => None,
        };
        let row = self.seal_row(value).await?;
        let key = self.keys().misc(name);
        let tx = crate::meta::Transaction::new()
            .expect(key.clone(), expected)
            .set(key, row);
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// [`RepoStore::set_misc`] that writes only while `lease` holds and the
    /// value is still `expected` (`None`: absent): a compare-and-swap.
    pub async fn set_misc_if(
        &self,
        name: &str,
        expected: Option<Bytes>,
        value: Bytes,
        lease: &super::leases::Lease,
    ) -> Result<()> {
        let expected = match expected {
            Some(old) => Some(self.seal_row(old).await?),
            None => None,
        };
        let row = self.seal_row(value).await?;
        let key = self.keys().misc(name);
        let mut tx = crate::meta::Transaction::new()
            .expect(key.clone(), expected)
            .set(key, row);
        tx.conditions.push(lease.fence(self));
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub(super) async fn seal_row(&self, value: Bytes) -> Result<Bytes> {
        if value.len() <= OFFLOAD_BYTES {
            let mut row = Vec::with_capacity(value.len() + 1);
            row.push(INLINE);
            row.extend_from_slice(&value);
            return Ok(Bytes::from(row));
        }
        let hash = ObjectHash::digest(ObjectKind::Derived, &value);
        self.objects()
            .put(ObjectKind::Derived, &hash, value)
            .await?;
        let mut row = vec![OBJECT];
        row.extend_from_slice(hash.as_bytes());
        Ok(Bytes::from(row))
    }

    pub(super) async fn open_row(&self, row: Option<Bytes>) -> Result<Option<Bytes>> {
        let Some(row) = row else {
            return Ok(None);
        };
        match row.first() {
            Some(&INLINE) => Ok(Some(row.slice(1..))),
            Some(&OBJECT) => {
                let hash = ObjectHash::from_slice(&row[1..])?;
                Ok(Some(
                    self.objects()
                        .get_required(ObjectKind::Derived, &hash)
                        .await?,
                ))
            }
            _ => Err(Error::Codec("malformed derived row".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Stores;

    use super::*;

    #[tokio::test]
    async fn large_values_round_trip_through_the_object_store() {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        let big = Bytes::from(vec![7u8; OFFLOAD_BYTES * 3]);
        repo.set_misc("big", big.clone()).await.unwrap();
        repo.set_misc("small", Bytes::from_static(b"x"))
            .await
            .unwrap();
        assert_eq!(repo.get_misc("big").await.unwrap(), Some(big));
        assert_eq!(
            repo.get_misc("small").await.unwrap(),
            Some(Bytes::from_static(b"x"))
        );
        let row = repo.meta().get(&repo.keys().misc("big")).await.unwrap();
        assert_eq!(row.unwrap().len(), 33);
    }

    #[tokio::test]
    async fn derived_values_batch_in_one_round_trip() {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("r", "").await.unwrap().id);
        let h = |i: u8| ObjectHash::digest(ObjectKind::Blob, &[i]);
        let big = Bytes::from(vec![1u8; OFFLOAD_BYTES + 1]);
        repo.put_derived_many(
            "k",
            &[(h(1), Bytes::from_static(b"a")), (h(2), big.clone())],
            None,
        )
        .await
        .unwrap();
        let got = repo
            .get_derived_many("k", &[h(1), h(3), h(2)])
            .await
            .unwrap();
        assert_eq!(got, vec![Some(Bytes::from_static(b"a")), None, Some(big)]);
    }
}
