//! Identity: one org's users, agents, sign-in providers, workload issuers
//! and SCIM credentials, and the instance's token keys, revocations and
//! kill switch.

mod agents;
mod providers;
mod tokens;
mod users;

use crate::client::Client;
use crate::names;

/// Identity operations on one org (org administrators, unless a method
/// says otherwise).
#[derive(Clone, Debug)]
pub struct IdentityClient {
    client: Client,
    /// The org's resource name.
    org: String,
}

impl IdentityClient {
    pub(crate) fn new(client: Client, org: &str) -> Self {
        IdentityClient {
            client,
            org: names::org(org),
        }
    }

    fn svc(&self) -> crate::client::IdentitySvc {
        self.client.identity_svc()
    }

    /// `orgs/{org}/{collection}/{id}`.
    fn child(&self, collection: &str, id: &str) -> String {
        format!("{}/{collection}/{id}", self.org)
    }

    fn request_id() -> String {
        names::request_id(&rill_types::OperationId::new())
    }
}

/// An update mask naming `paths`.
fn mask(paths: &[&str]) -> rill_proto::FieldMask {
    rill_proto::FieldMask {
        paths: paths.iter().map(|p| p.to_string()).collect(),
        ..Default::default()
    }
}
