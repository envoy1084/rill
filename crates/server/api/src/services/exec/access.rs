//! Who may use the execution service, and for which org.

use rill_exec::remote::ExecSpace;
use rill_policy::{AuthContext, Verb};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;
use tonic::Request;

use crate::auth::{administers_org, caller, is_worker};

/// The caller of an execution request and the space it acts in.
pub struct ExecCaller {
    pub space: ExecSpace,
    pub ctx: AuthContext,
    pub worker: bool,
}

/// The org an instance name names: the empty name is the default org.
pub fn org_of(instance_name: &str) -> &str {
    let name = instance_name.trim_matches('/');
    if name.is_empty() {
        rill_store::DEFAULT_ORG
    } else {
        name
    }
}

/// Admit the caller of `request` to the execution space of
/// `instance_name`: a worker, anyone in dev mode, or a member of the org
/// whose token, if narrowed, carries the execute verb.
pub async fn admit<T>(
    state: &AppState,
    request: &Request<T>,
    instance_name: &str,
) -> Result<ExecCaller> {
    if !state.config.exec.enabled {
        return Err(Error::Unimplemented("remote execution is disabled".into()));
    }
    let ctx = caller(request)?;
    let worker = is_worker(request);
    let org = org_of(instance_name);
    if org != rill_store::DEFAULT_ORG && state.stores.get_org(org).await?.is_none() {
        return Err(Error::not_found(format!("instance {instance_name:?}")));
    }
    if !worker && !state.dev_mode() {
        let member = ctx.claims.as_ref().is_some_and(|c| c.org == org)
            || administers_org(&state.identity, false, &ctx, org).await?;
        if !member {
            return Err(Error::denied(format!(
                "{} may not execute in {org:?}",
                ctx.principal
            )));
        }
        if let Some(cap) = &ctx.capability {
            if !cap.verbs.contains(&Verb::Execute) {
                return Err(Error::denied("the token does not carry the execute verb"));
            }
        }
    }
    Ok(ExecCaller {
        space: state.exec.space(org),
        ctx,
        worker: worker || state.dev_mode(),
    })
}
