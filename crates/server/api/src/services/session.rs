//! The session service: signing in over gRPC, for clients that hold no
//! token yet. Served without the auth layer.

use std::sync::Arc;

use rill_identity::signin::{RefreshToken, SessionTokens};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::session_service_server::SessionService;
use tonic::{Request, Response, Status};

use crate::signin::flows;
use rill_runtime::error::Error;
use rill_runtime::state::AppState;

const TOKEN_TYPE_ID: &str = "urn:ietf:params:oauth:token-type:id_token";
const TOKEN_TYPE_JWT: &str = "urn:ietf:params:oauth:token-type:jwt";

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

/// The session service.
#[derive(Clone)]
pub struct SessionSvc {
    state: Arc<AppState>,
}

impl SessionSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        SessionSvc { state }
    }
}

fn grant(tokens: SessionTokens) -> pb::SessionGrant {
    pb::SessionGrant {
        access_token: tokens.access_token,
        expires_at: timestamp_to_proto(tokens.expires_at),
        refresh_token: Some(tokens.refresh_token.to_string()),
        session: tokens.session.id.to_string(),
        principal: Some(pb::Principal::from(&tokens.session.principal)),
        org: tokens.session.org,
    }
}

#[tonic::async_trait]
impl SessionService for SessionSvc {
    async fn list_sign_in_providers(
        &self,
        request: Request<pb::ListSignInProvidersRequest>,
    ) -> GrpcResult<pb::ListSignInProvidersResponse> {
        let org = request.into_inner().org;
        let org = if org.is_empty() {
            rill_store::DEFAULT_ORG.to_string()
        } else {
            org
        };
        let providers = flows::sign_in_providers(&self.state, &org)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(pb::ListSignInProvidersResponse {
            providers: providers
                .into_iter()
                .map(|p| pb::SignInProvider {
                    org: p.org,
                    name: p.name,
                    issuer: p.issuer,
                    cli_client_id: p.cli_client_id,
                    scopes: p.scopes,
                    device_authorization_endpoint: p.device_authorization_endpoint,
                    token_endpoint: p.token_endpoint,
                })
                .collect(),
        }))
    }

    async fn exchange_token(
        &self,
        request: Request<pb::ExchangeTokenRequest>,
    ) -> GrpcResult<pb::SessionGrant> {
        let req = request.into_inner();
        let result = match req.subject_token_type.as_str() {
            TOKEN_TYPE_ID => flows::exchange_id_token(&self.state, &req.subject_token)
                .await
                .map(grant),
            TOKEN_TYPE_JWT => {
                let org = if req.org.is_empty() {
                    rill_store::DEFAULT_ORG
                } else {
                    req.org.as_str()
                };
                flows::exchange_workload(
                    &self.state,
                    &req.subject_token,
                    org,
                    req.agent.as_deref(),
                    req.model.as_deref(),
                )
                .await
                .map(|g| pb::SessionGrant {
                    access_token: g.access_token,
                    expires_at: timestamp_to_proto(g.expires_at),
                    refresh_token: None,
                    session: g.session.id.to_string(),
                    principal: Some(pb::Principal::from(&g.session.principal)),
                    org: g.session.org,
                })
            }
            other => Err(Error::invalid(format!(
                "unsupported subject_token_type {other:?}"
            ))),
        };
        result.map(Response::new).map_err(Status::from)
    }

    async fn refresh_session(
        &self,
        request: Request<pb::RefreshSessionRequest>,
    ) -> GrpcResult<pb::SessionGrant> {
        let result = async {
            let token: RefreshToken = request.get_ref().refresh_token.parse()?;
            flows::refresh_live(&self.state, &token).await
        }
        .await;
        result
            .map(|t| Response::new(grant(t)))
            .map_err(Status::from)
    }
}
