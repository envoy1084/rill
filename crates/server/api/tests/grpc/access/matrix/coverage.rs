//! Every RPC called as every role gets exactly what its class allows.

use tonic::Code;

use super::super::raw;
use super::callers::callers;
use super::classes::class_of;
use super::leaks::assert_no_leak;
use super::probes::targeted;
use super::roles::{passes, refused, Role};
use crate::*;

#[tokio::test(flavor = "multi_thread")]
async fn every_caller_gets_exactly_what_its_class_allows() {
    let callers = callers().await;
    let channel = channel(&callers.acl.server.grpc).await;
    let mut mismatches = Vec::new();
    for method in raw::methods() {
        let class = class_of(&method);
        for (role, token) in &callers.tokens {
            let Some(expected) = passes(class, *role) else {
                continue;
            };
            let probe = targeted(&method.name, &callers);
            let precise = probe.is_some();
            let body =
                probe.unwrap_or_else(|| raw::request_body(&method, "acl", &callers.workspace));
            let outcome = raw::call(channel.clone(), &method, body, token.as_deref()).await;
            let passed = !refused(outcome.code);
            // An empty request may fail validation before the permission
            // check; that is not a pass, but such an RPC needs a targeted
            // probe.
            let validated_first = !expected
                && !precise
                && matches!(
                    outcome.code,
                    Code::InvalidArgument | Code::NotFound | Code::FailedPrecondition
                );
            if *role == Role::Anonymous {
                if outcome.code != Code::Unauthenticated {
                    mismatches.push(format!("{} as anonymous: {:?}", method.name, outcome.code));
                }
                continue;
            }
            if validated_first {
                mismatches.push(format!(
                    "{} as {role:?} answered {:?} before any permission check; add a targeted probe",
                    method.name, outcome.code
                ));
            } else if passed != expected {
                mismatches.push(format!(
                    "{} as {role:?}: expected {}, got {:?}: {}",
                    method.name,
                    if expected { "through" } else { "refused" },
                    outcome.code,
                    String::from_utf8_lossy(&outcome.body)
                        .chars()
                        .take(120)
                        .collect::<String>()
                ));
            }
            if matches!(
                role,
                Role::PubReader | Role::PubWriter | Role::Narrowed | Role::Stranger
            ) {
                assert_no_leak(&format!("{} as {role:?}", method.name), &outcome);
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    callers.acl.server.stop().await;
}
