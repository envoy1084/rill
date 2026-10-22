//! The repository the trunk-red scenarios break: a verify plan with two
//! checks a change breaks by writing `BROKEN` into their files, a test
//! that flakes on demand, and a steady trickle of unrelated landings.

use std::time::{Duration, Instant};

use bytes::Bytes;
use rill_proto::rill::v1 as pb;
use rill_sdk::{ApplyOptions, Client, Edit, EditBase, LandStatus, RepoClient, Root};
use rill_types::{FileKind, RepoPathBuf, SparseProfile, Tree, TrunkSeq};

/// How often the scenarios look at trunk.
pub const POLL: Duration = Duration::from_millis(200);

/// `a` and `b` fail while any of their files says `BROKEN`. `unit`
/// writes a JUnit report: in `retry-*` mode its one test passes after a
/// retry, in `flaky-*` mode it fails the first time it runs on that mode
/// and passes after (a marker outside the sandbox remembers), else it
/// passes.
fn plan(markers: &str) -> String {
    let report = |case: &str| {
        format!(
            r#"printf '%s' '<testsuites><testsuite name="suite"><testcase name="flaky" classname="suite">{case}</testcase></testsuite></testsuites>' > report.xml"#
        )
    };
    let unit = format!(
        "mode=$(cat flaky/mode); case $mode in \
         retry-*) {retried};; \
         flaky-*) if test -e {markers}/$mode; then {pass}; else touch {markers}/$mode; {fail}; exit 1; fi;; \
         *) {pass};; esac",
        retried = report(r#"<flakyFailure message="x"/>"#),
        pass = report(""),
        fail = report(r#"<failure message="x"/>"#),
    );
    let check = |name: &str| {
        format!(
            "[[step]]\nname = \"{name}\"\ninputs = [\"{name}/**\"]\n\
             command = [\"sh\", \"-c\", \"! grep -q BROKEN {name}/*\"]\n\n"
        )
    };
    format!(
        "{}{}[[step]]\nname = \"unit\"\ninputs = [\"flaky/**\"]\njunit = \"report.xml\"\n\
         command = [\"sh\", \"-c\", '''{unit}''']\n",
        check("a"),
        check("b")
    )
}

/// A file to write.
pub fn set(path: &str, body: impl Into<Bytes>) -> Edit {
    Edit::Set {
        path: RepoPathBuf::new(path).expect("scenario paths are valid"),
        bytes: body.into(),
        kind: FileKind::Regular,
    }
}

/// The repository under test.
#[derive(Clone)]
pub struct Trunk {
    pub client: Client,
    pub repo: RepoClient,
    pub name: String,
}

impl Trunk {
    /// Create `name` and land the plan and its files; `markers` is a
    /// directory the flaky test may write.
    pub async fn seed(client: Client, name: &str, markers: &str) -> anyhow::Result<Self> {
        client
            .create_repo(name, Some("trunk-red gate".into()))
            .await?;
        let repo = client.repo(name);
        repo.workspaces()
            .create("seed", None, SparseProfile::full())
            .await?;
        let trunk = Trunk {
            client,
            repo,
            name: name.into(),
        };
        let edits = vec![
            set(".rill/verify.toml", plan(markers)),
            set("a/seed.txt", "fine\n"),
            set("b/seed.txt", "fine\n"),
            set("flaky/mode", "pass\n"),
        ];
        trunk
            .land_at(Root::Tree(Tree::empty_hash()), edits, "seed")
            .await?;
        Ok(trunk)
    }

    /// Land `edits` at trunk's head; the position it landed at.
    pub async fn land(&self, edits: Vec<Edit>, message: &str) -> anyhow::Result<TrunkSeq> {
        self.land_at(Root::trunk(), edits, message).await
    }

    async fn land_at(
        &self,
        base: Root,
        edits: Vec<Edit>,
        message: &str,
    ) -> anyhow::Result<TrunkSeq> {
        let result = self
            .repo
            .apply_edits(EditBase::Root(base), edits, ApplyOptions::message(message))
            .await?;
        let change = result.version.change_id;
        let reviews = self.repo.reviews();
        reviews.submit(change).await?;
        reviews.vote(change, "Review", 2, None, None).await?;
        reviews.vote(change, "Verified", 1, None, None).await?;
        // The seed and plan edits are high risk, which needs an owner too.
        reviews.vote(change, "Owner", 1, None, None).await?;
        let ticket = reviews.request_land(change).await?;
        let landed = self
            .repo
            .stack()
            .await_land(&ticket.id, Duration::from_secs(120))
            .await?;
        anyhow::ensure!(
            landed.status() == LandStatus::Landed,
            "{message:?} did not land"
        );
        landed
            .seq
            .ok_or_else(|| anyhow::anyhow!("{message:?} landed without a position"))
    }

    /// Trunk's health and its open incidents.
    pub async fn health(&self) -> anyhow::Result<(pb::TrunkHealth, Vec<pb::Incident>)> {
        Ok(self.repo.trunk().health().await?)
    }

    /// The latest incidents, open or closed, newest first.
    pub async fn incidents(&self) -> anyhow::Result<Vec<pb::Incident>> {
        Ok(self.repo.trunk().incidents(None, 100, None).await?.items)
    }

    /// The incident whose culprit is `seq`, open or closed.
    pub async fn incident(&self, seq: TrunkSeq) -> anyhow::Result<Option<pb::Incident>> {
        Ok(self
            .incidents()
            .await?
            .into_iter()
            .find(|i| i.culprit == seq.get()))
    }

    /// Wait until `seq` has a verification outcome.
    pub async fn verified(&self, seq: TrunkSeq, within: Duration) -> anyhow::Result<()> {
        use rill_sdk::VerificationStatus::{Pending, Running};
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            let record = self.repo.trunk().verification(seq).await?;
            if !matches!(record.status, Pending | Running) {
                return Ok(());
            }
            tokio::time::sleep(POLL).await;
        }
        anyhow::bail!("trunk@{seq} was not verified within {within:?}")
    }

    /// Wait until trunk is green with no incident open.
    pub async fn green(&self, within: Duration) -> anyhow::Result<()> {
        let deadline = Instant::now() + within;
        loop {
            let (health, open) = self.health().await?;
            if health.state == "green" && open.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let open: Vec<_> = open
                    .iter()
                    .map(|i| (i.culprit, i.state.clone(), i.resolution.clone()))
                    .collect();
                anyhow::bail!(
                    "trunk was not green within {within:?}: {}, open {open:?}",
                    health.state
                );
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Land an unrelated change every `every` until dropped: trunk keeps
    /// moving under the scenarios, as it would.
    pub fn trickle(&self, every: Duration) -> tokio::task::JoinHandle<()> {
        let trunk = self.clone();
        tokio::spawn(async move {
            let mut n = 0u64;
            loop {
                n += 1;
                let edits = vec![set(&format!("bg/{n}.txt"), format!("{n}\n"))];
                if let Err(e) = trunk.land(edits, &format!("background {n}")).await {
                    eprintln!("background landing {n}: {e}");
                }
                tokio::time::sleep(every).await;
            }
        })
    }
}
