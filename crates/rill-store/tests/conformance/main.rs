//! Backend conformance: every scenario on every backend. Any backend that
//! diverges from the others here is wrong.
//!
//! With the `fdb` feature the FoundationDB backend joins, using the cluster
//! file in `RILL_FDB_CLUSTER_FILE` (or the platform default) and a fresh
//! root prefix per fixture.

mod layer;
mod meta;
mod objects;
mod race;
mod repo;

use std::sync::Arc;

use bytes::Bytes;
use rill_store::{MemoryMetaStore, MemoryObjectStore, MetaStore, Stores};

pub fn b(s: &[u8]) -> Bytes {
    Bytes::copy_from_slice(s)
}

/// One backend pair. Anything the backend needs kept alive or cleaned up
/// rides along.
struct Fixture {
    name: &'static str,
    stores: Stores,
    _dir: Option<tempfile::TempDir>,
    #[cfg(feature = "fdb")]
    fdb: Option<Arc<rill_store::FdbMetaStore>>,
}

impl Fixture {
    fn new(name: &'static str, stores: Stores) -> Self {
        Fixture {
            name,
            stores,
            _dir: None,
            #[cfg(feature = "fdb")]
            fdb: None,
        }
    }

    fn meta(&self) -> &Arc<dyn MetaStore> {
        &self.stores.meta
    }

    async fn finish(self) {
        #[cfg(feature = "fdb")]
        if let Some(fdb) = self.fdb {
            fdb.engine().clear_all().await.unwrap();
        }
    }
}

fn memory_objects() -> Arc<MemoryObjectStore> {
    Arc::new(MemoryObjectStore::new())
}

async fn fixtures() -> Vec<Fixture> {
    let mut out = vec![Fixture::new(
        "memory",
        Stores::new(memory_objects(), Arc::new(MemoryMetaStore::new())),
    )];
    #[cfg(feature = "local")]
    {
        let dir = tempfile::tempdir().unwrap();
        let mut local = Fixture::new(
            "local",
            rill_store::LocalStores::open(dir.path()).await.unwrap(),
        );
        local._dir = Some(dir);
        out.push(local);
        out.push(Fixture::new(
            "redb-temporary",
            Stores::new(
                memory_objects(),
                Arc::new(rill_store::RedbMetaStore::temporary().unwrap()),
            ),
        ));
    }
    #[cfg(feature = "s3")]
    out.push(Fixture::new(
        "object_store",
        Stores::new(
            Arc::new(rill_store::ObjectStoreBackend::new(
                Arc::new(object_store::memory::InMemory::new()),
                "rill",
            )),
            Arc::new(MemoryMetaStore::new()),
        ),
    ));
    #[cfg(feature = "fdb")]
    {
        let cluster = std::env::var("RILL_FDB_CLUSTER_FILE").ok();
        let root = format!("rill-test-{}", hex::encode(rill_types::ids::random_id()));
        let fdb = Arc::new(
            rill_store::FdbMetaStore::connect(cluster.as_deref(), &root)
                .expect("the fdb feature needs a running FoundationDB cluster"),
        );
        let mut f = Fixture::new("fdb", Stores::new(memory_objects(), fdb.clone()));
        f.fdb = Some(fdb);
        out.push(f);
    }
    out
}

#[tokio::test]
async fn object_stores_conform() {
    for f in fixtures().await {
        objects::scenario(f.name, &f.stores.objects).await;
        f.finish().await;
    }
}

#[tokio::test]
async fn meta_stores_conform() {
    for f in fixtures().await {
        meta::conditions(f.name, f.meta()).await;
        meta::append_log(f.name, f.meta()).await;
        meta::scan(f.name, f.meta()).await;
        meta::prefix_bounds(f.name, f.meta()).await;
        f.finish().await;
    }
}

#[tokio::test]
async fn keyspace_layer_conforms() {
    for f in fixtures().await {
        layer::stamps(f.name, f.meta()).await;
        layer::counters(f.name, f.meta()).await;
        layer::split_values(f.name, f.meta()).await;
        layer::budget(f.name, f.meta()).await;
        f.finish().await;
    }
}

#[tokio::test]
async fn repo_stores_conform() {
    for f in fixtures().await {
        repo::scenario(f.name, &f.stores).await;
        f.finish().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_writers_conform() {
    for f in fixtures().await {
        race::compare_and_swap(f.name, f.meta()).await;
        race::landing(f.name, &f.stores).await;
        race::blind_writes(f.name, f.meta()).await;
        race::duplicate_puts(f.name, &f.stores).await;
        f.finish().await;
    }
}

#[tokio::test]
async fn stores_refuse_another_format() {
    for f in fixtures().await {
        f.stores.check_format().await.unwrap();
        f.stores.check_format().await.unwrap();
        f.meta()
            .set(
                rill_store::keys::store_format(),
                Bytes::copy_from_slice(&(rill_store::STORE_FORMAT + 1).to_be_bytes()),
            )
            .await
            .unwrap();
        let err = f.stores.check_format().await.unwrap_err();
        assert!(
            matches!(err, rill_store::Error::IncompatibleFormat { .. }),
            "{}: {err}",
            f.name
        );
        f.finish().await;
    }
}
