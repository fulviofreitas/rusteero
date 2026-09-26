//! `Client` methods for the `MembersAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/members.py` (v8.0.4) by way of `EeroClient`'s own
//! `members`-scoped wrappers in `client.py`. None of the ten wrappers below passes a `parent` to
//! its domain call (`.claude/tasks/briefs/v8/client.md` §4's `members` table carries no `+net`
//! note on any row) — `EeroClient` has no `_member_parent_kwargs` helper, matching every single-
//! resource read/write elsewhere in `client.py` that is called with no `parent=` at all (brief
//! §3.3).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Lists network members — returns the raw Eero API response.
    ///
    /// Ported from `get_members()` (`client.py:2574-2580`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_members(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.members().get_members(&network_id, None).await
    }

    /// Lists pending invites — returns the raw Eero API response.
    ///
    /// Ported from `get_invites()` (`client.py:2581-2585`). `auto_discover = false`. Unverified
    /// live — access denied on some accounts, see
    /// [`crate::endpoints::members::MembersApi::get_invites`]'s own docs.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_invites(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.members().get_invites(&network_id).await
    }

    /// Creates an invite — returns the raw Eero API response.
    ///
    /// Ported from `create_invite()` (`client.py:2586-2590`). `auto_discover = false`.
    /// Invalidates nothing: `members` has no cache bucket at all (behaviour brief §2.1), and
    /// `client.py`'s own method body has no `del self._cache[...]` call to reproduce.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "role", .. }` if `role` (after normalisation) is not
    /// `"admin"`/`"owner"` — see
    /// [`crate::endpoints::members::MembersApi::create_invite`]. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn create_invite(
        &self,
        role: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.members().create_invite(&network_id, role).await
    }

    /// Updates an invite's nickname — returns the raw Eero API response.
    ///
    /// Ported from `update_invite()` (`client.py:2591-2599`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn update_invite(
        &self,
        invite_id: &str,
        invite_nickname: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .members()
            .update_invite(&network_id, invite_id, invite_nickname)
            .await
    }

    /// Deletes an invite — returns the raw Eero API response.
    ///
    /// Ported from `delete_invite()` (`client.py:2600-2606`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_invite(
        &self,
        invite_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .members()
            .delete_invite(&network_id, invite_id)
            .await
    }

    /// Accepts or rejects an invite — returns the raw Eero API response.
    ///
    /// Ported from `respond_to_invite()` (`client.py:2607-2620`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "invite_id", .. }` if exactly one of `invite_id`/
    /// `invite_code` is not supplied — see
    /// [`crate::endpoints::members::MembersApi::respond_to_invite`]. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn respond_to_invite(
        &self,
        accept: bool,
        invite_id: Option<&str>,
        invite_code: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .members()
            .respond_to_invite(&network_id, accept, invite_id, invite_code)
            .await
    }

    /// Cancels pending admin invites — returns the raw Eero API response.
    ///
    /// Ported from `cancel_pending_admin()` (`client.py:2621-2625`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn cancel_pending_admin(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.members().cancel_pending_admin(&network_id).await
    }

    /// Promotes a member to admin — returns the raw Eero API response.
    ///
    /// Ported from `promote_member()` (`client.py:2626-2632`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn promote_member(
        &self,
        member_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .members()
            .promote_member(&network_id, member_id)
            .await
    }

    /// Removes an admin — returns the raw Eero API response.
    ///
    /// Ported from `remove_admin()` (`client.py:2633-2637`). `auto_discover = false`.
    /// Invalidates nothing — see [`Client::create_invite`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn remove_admin(
        &self,
        user_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.members().remove_admin(&network_id, user_id).await
    }

    /// Looks up an invite by its code — returns the raw Eero API response.
    ///
    /// Ported from `query_invite()` (`client.py:2638-2640`). **Not network-scoped** — the only
    /// `MembersAPI` wrapper with no `network_id` parameter at all, matching
    /// [`crate::endpoints::members::MembersApi::query_invite`] itself.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`], minus the `Error::MissingNetworkId` case (no network
    /// resolution happens here at all).
    pub async fn query_invite(&self, invite_code: &str) -> Result<Envelope, Error> {
        self.api.members().query_invite(invite_code).await
    }
}
