//! An org's users.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;

use super::{mask, IdentityClient};
use crate::client::retry_idempotent;
use crate::error::Result;
use crate::paging;

impl IdentityClient {
    /// The org's users matching `filter` (on `status`, `roles`, `teams`;
    /// empty for all).
    pub async fn users(&self, filter: &str) -> Result<Vec<pb::User>> {
        paging::all(|page_token| async move {
            let request = pb::ListUsersRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
                filter: filter.to_string(),
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_users(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.users, resp.next_page_token))
        })
        .await
    }

    /// Suspend or reactivate user `name`; suspension refuses every token
    /// they hold at once.
    pub async fn set_user_suspended(&self, name: &str, suspended: bool) -> Result<pb::User> {
        let user = pb::User {
            name: self.child("users", name),
            status: match suspended {
                true => "suspended",
                false => "active",
            }
            .into(),
            ..Default::default()
        };
        self.update_user(user, &["status"]).await
    }

    /// Replace user `name`'s roles (`admin`, or none).
    pub async fn set_user_roles(&self, name: &str, roles: Vec<String>) -> Result<pb::User> {
        let user = pb::User {
            name: self.child("users", name),
            roles,
            ..Default::default()
        };
        self.update_user(user, &["roles"]).await
    }

    async fn update_user(&self, user: pb::User, paths: &[&str]) -> Result<pb::User> {
        let request = pb::UpdateUserRequest {
            request_id: Self::request_id(),
            user: user.into(),
            update_mask: mask(paths).into(),
            etag: String::new(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_user(request).await }
        })
        .await?
        .into_owned();
        Ok(required("update_user.user", resp.user)?)
    }
}
