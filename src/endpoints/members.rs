//! `MembersApi`: `members` endpoints (`eero-api src/eero/api/members.py`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/members.py` (v8.0.4). See
//! `.claude/tasks/briefs/v8/g7-backup-members.md` §1 for the per-method table this file is
//! scoped by.
//!
//! Every method here funnels through `Transport::resource`/`Transport::nested`, which already
//! implement the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// Valid `role` values for [`MembersApi::create_invite`], sorted — the exact set and order
/// `_INVITE_ROLES` (`members.py:28`) reports in its own validation-error message.
///
/// Ported from `_INVITE_ROLES` (`members.py:28`).
const INVITE_ROLES: &[&str] = &["admin", "owner"];

/// `eero-api`'s `MembersAPI` (`src/eero/api/members.py`, new in v8.0.0).
///
/// Build one with [`MembersApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `MembersApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct MembersApi {
    transport: Arc<Transport>,
}

impl MembersApi {
    /// Wraps `transport` as a `MembersApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/members` — list network members.
    ///
    /// Ported from `MembersAPI.get_members` (`members.py:48-80`). Prefers `parent`'s own
    /// published `members` link over the literal template when supplied
    /// (`routes::MEMBERS_GET_MEMBERS::link`). Returns the raw `{"meta": …, "data": {...}}`
    /// envelope; this method never inspects or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::resource`.
    pub async fn get_members(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::MEMBERS_GET_MEMBERS,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/invites` — list pending invites.
    ///
    /// Ported from `MembersAPI.get_invites` (`members.py:82-103`). Unverified live — some
    /// accounts see an access-denied response here (module docstring, `members.py:6-8`).
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn get_invites(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::MEMBERS_GET_INVITES,
                network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/invites` — create an invite.
    ///
    /// Ported from `MembersAPI.create_invite` (`members.py:105-138`). `role` is normalised with
    /// `.trim().to_lowercase()` (`role.strip().lower()`, `members.py:129`) before both the
    /// membership check and the wire value — `"Admin"` sends `"admin"`
    /// (`test_sends_invite_role[Admin-admin]`). Sends `{"invite_role": normalised}`
    /// (`members.py:138`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "role", .. }` if the normalised role is not one of
    /// `"admin"`/`"owner"`, before any request is sent. Otherwise see [`Self::get_members`].
    pub async fn create_invite(&self, network_id: &str, role: &str) -> Result<Envelope, Error> {
        let normalised = role.trim().to_lowercase();
        if !INVITE_ROLES.contains(&normalised.as_str()) {
            return Err(Error::validation(
                "role",
                format!("must be one of ['admin', 'owner'], got '{role}'"),
            ));
        }
        let url =
            routes::MEMBERS_CREATE_INVITE.resolve(self.transport.api_host(), network_id, None)?;
        crate::links::warn_uncharacterised_write("create invite for network");
        self.transport
            .request(
                routes::MEMBERS_CREATE_INVITE.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "invite_role": normalised })),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/invites/{invite_id}` — update an invite's nickname.
    ///
    /// Ported from `MembersAPI.update_invite` (`members.py:140-166`). Sends
    /// `{"invite_nickname": invite_nickname}` (`members.py:166`). The identifier is never logged
    /// — the warning below never interpolates `invite_id` into its fixed operation string
    /// (`members.py:165`, `.claude/tasks/briefs/v8/g7-backup-members.md` §3's "sensitive-value
    /// handling" note).
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn update_invite(
        &self,
        network_id: &str,
        invite_id: &str,
        invite_nickname: &str,
    ) -> Result<Envelope, Error> {
        let url = routes::MEMBERS_UPDATE_INVITE.resolve(
            self.transport.api_host(),
            network_id,
            invite_id,
            None,
        )?;
        crate::links::warn_uncharacterised_write("update invite for network");
        self.transport
            .request(
                routes::MEMBERS_UPDATE_INVITE.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "invite_nickname": invite_nickname })),
            )
            .await
    }

    /// `DELETE /2.2/networks/{network_id}/invites/{invite_id}` — delete an invite.
    ///
    /// Ported from `MembersAPI.delete_invite` (`members.py:168-190`). The identifier is never
    /// logged — see [`Self::update_invite`].
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn delete_invite(
        &self,
        network_id: &str,
        invite_id: &str,
    ) -> Result<Envelope, Error> {
        let url = routes::MEMBERS_DELETE_INVITE.resolve(
            self.transport.api_host(),
            network_id,
            invite_id,
            None,
        )?;
        crate::links::warn_uncharacterised_write("delete invite for network");
        self.transport
            .request(
                routes::MEMBERS_DELETE_INVITE.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/invites/response` — accept or reject an invite.
    ///
    /// Ported from `MembersAPI.respond_to_invite` (`members.py:192-243`). Sends `{"accept":
    /// accept}` plus exactly one of `{"invite_id": ..}`/`{"invite_code": ..}`
    /// (`members.py:236-241`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "invite_id", .. }` if `invite_id` and `invite_code`
    /// are both `None` or both `Some` — the same field name Python's XOR check always reports,
    /// even when the actual problem is that `invite_code` was also supplied
    /// (`members.py:225-228`). Otherwise see [`Self::get_members`].
    pub async fn respond_to_invite(
        &self,
        network_id: &str,
        accept: bool,
        invite_id: Option<&str>,
        invite_code: Option<&str>,
    ) -> Result<Envelope, Error> {
        if invite_id.is_none() == invite_code.is_none() {
            return Err(Error::validation(
                "invite_id",
                "exactly one of invite_id or invite_code must be supplied",
            ));
        }
        let mut body = Map::new();
        body.insert("accept".to_owned(), Value::Bool(accept));
        if let Some(invite_id) = invite_id {
            body.insert("invite_id".to_owned(), Value::String(invite_id.to_owned()));
        }
        if let Some(invite_code) = invite_code {
            body.insert(
                "invite_code".to_owned(),
                Value::String(invite_code.to_owned()),
            );
        }
        let url = routes::MEMBERS_RESPOND_TO_INVITE.resolve(
            self.transport.api_host(),
            network_id,
            None,
        )?;
        crate::links::warn_uncharacterised_write("respond to invite for network");
        self.transport
            .request(
                routes::MEMBERS_RESPOND_TO_INVITE.method.clone(),
                url,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/invites/cancel_pending_admin` — cancel pending admin
    /// invites.
    ///
    /// Ported from `MembersAPI.cancel_pending_admin` (`members.py:245-273`). Sends the literal
    /// two-byte body `""` (`RequestBody::EmptyJsonString`), matching
    /// `RequestEncoding.EMPTY_JSON_STRING` (`members.py:270-272`) — **not** an empty JSON object
    /// `{}`.
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn cancel_pending_admin(&self, network_id: &str) -> Result<Envelope, Error> {
        let url = routes::MEMBERS_CANCEL_PENDING_ADMIN.resolve(
            self.transport.api_host(),
            network_id,
            None,
        )?;
        crate::links::warn_uncharacterised_write("cancel pending admin invites for network");
        self.transport
            .request(
                routes::MEMBERS_CANCEL_PENDING_ADMIN.method.clone(),
                url,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/member_promotion` — promote a member to admin.
    ///
    /// Ported from `MembersAPI.promote_member` (`members.py:274-300`). Sends `{"member_id":
    /// member_id}` (`members.py:300`).
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn promote_member(
        &self,
        network_id: &str,
        member_id: &str,
    ) -> Result<Envelope, Error> {
        let url =
            routes::MEMBERS_PROMOTE_MEMBER.resolve(self.transport.api_host(), network_id, None)?;
        crate::links::warn_uncharacterised_write("promote member for network");
        self.transport
            .request(
                routes::MEMBERS_PROMOTE_MEMBER.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "member_id": member_id })),
            )
            .await
    }

    /// `DELETE /2.2/networks/{network_id}/admins/{user_id}` — remove an admin.
    ///
    /// Ported from `MembersAPI.remove_admin` (`members.py:302-324`). The identifier is never
    /// logged — see [`Self::update_invite`].
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn remove_admin(&self, network_id: &str, user_id: &str) -> Result<Envelope, Error> {
        let url = routes::MEMBERS_REMOVE_ADMIN.resolve(
            self.transport.api_host(),
            network_id,
            user_id,
            None,
        )?;
        crate::links::warn_uncharacterised_write("remove admin from network");
        self.transport
            .request(
                routes::MEMBERS_REMOVE_ADMIN.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/inviteQuery` — look up an invite by its code.
    ///
    /// **Not network-scoped** — the only method in this domain with no `network_id` parameter at
    /// all (module docs, `.claude/tasks/briefs/v8/g7-backup-members.md` §3). Ported from
    /// `MembersAPI.query_invite` (`members.py:326-353`). Sends `{"invite_code": invite_code}`
    /// (`members.py:352`). `invite_code` is never logged — the warning below never interpolates
    /// it into its fixed operation string (`members.py:348`).
    ///
    /// # Errors
    ///
    /// See [`Self::get_members`].
    pub async fn query_invite(&self, invite_code: &str) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("query invite by code");
        self.transport
            .resource(
                &routes::MEMBERS_QUERY_INVITE,
                "",
                None,
                &[],
                RequestBody::Json(json!({ "invite_code": invite_code })),
            )
            .await
    }
}
