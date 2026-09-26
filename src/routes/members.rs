//! `members` routes (`MembersAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/members.py` (v8.0.4). See
//! `.claude/tasks/briefs/v8/g7-backup-members.md` §1 for the per-method URL-resolution table
//! these constants are drawn from. Every route here is at `API_VERSION_DEFAULT` ("2.2"), the
//! same default every `resource_url`/`sub_resource_url` call in `members.py` uses
//! (`.claude/tasks/briefs/v8/g7-backup-members.md`, "Helper primitives" section).

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/members` — list network members.
///
/// Published as the `members` link on a network envelope (verified live, `test_prefers_parent_link`).
/// Ported from `eero-api src/eero/api/members.py:48` (`MembersAPI.get_members`).
pub const MEMBERS_GET_MEMBERS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/members",
    link: Some("members"),
};

/// `GET /2.2/networks/{network_id}/invites` — list pending invites.
///
/// Unverified — access denied on some accounts (module docstring, `members.py:6-8`). Ported
/// from `eero-api src/eero/api/members.py:82` (`MembersAPI.get_invites`).
pub const MEMBERS_GET_INVITES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/invites",
    link: None,
};

/// `POST /2.2/networks/{network_id}/invites` — create an invite.
///
/// Same collection resource as [`MEMBERS_GET_INVITES`]. Ported from `eero-api
/// src/eero/api/members.py:105` (`MembersAPI.create_invite`).
pub const MEMBERS_CREATE_INVITE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/invites",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/invites/{invite_id}` — update an invite's nickname.
///
/// Ported from `eero-api src/eero/api/members.py:140` (`MembersAPI.update_invite`).
pub const MEMBERS_UPDATE_INVITE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "invites",
    suffix: "",
    link: None,
};

/// `DELETE /2.2/networks/{network_id}/invites/{invite_id}` — delete an invite.
///
/// Ported from `eero-api src/eero/api/members.py:168` (`MembersAPI.delete_invite`).
pub const MEMBERS_DELETE_INVITE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "invites",
    suffix: "",
    link: None,
};

/// `POST /2.2/networks/{network_id}/invites/response` — accept or reject an invite.
///
/// Ported from `eero-api src/eero/api/members.py:192` (`MembersAPI.respond_to_invite`).
pub const MEMBERS_RESPOND_TO_INVITE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/invites/response",
    link: None,
};

/// `POST /2.2/networks/{network_id}/invites/cancel_pending_admin` — cancel pending admin
/// invites.
///
/// Ported from `eero-api src/eero/api/members.py:245` (`MembersAPI.cancel_pending_admin`).
pub const MEMBERS_CANCEL_PENDING_ADMIN: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/invites/cancel_pending_admin",
    link: None,
};

/// `POST /2.2/networks/{network_id}/member_promotion` — promote a member to admin.
///
/// Ported from `eero-api src/eero/api/members.py:274` (`MembersAPI.promote_member`).
pub const MEMBERS_PROMOTE_MEMBER: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/member_promotion",
    link: None,
};

/// `DELETE /2.2/networks/{network_id}/admins/{user_id}` — remove an admin.
///
/// Ported from `eero-api src/eero/api/members.py:302` (`MembersAPI.remove_admin`).
pub const MEMBERS_REMOVE_ADMIN: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "admins",
    suffix: "",
    link: None,
};

/// `POST /2.2/inviteQuery` — look up an invite by its code.
///
/// **Not network-scoped**: a fixed relative path under `API_ENDPOINT`, resolved against the
/// host directly (`template` carries no `{id}` placeholder, so [`Resource::resolve`] ignores
/// whatever `id_or_url` is passed and never touches `network_id` at all). Ported from `eero-api
/// src/eero/api/members.py:326` (`MembersAPI.query_invite`).
pub const MEMBERS_QUERY_INVITE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "inviteQuery",
    link: None,
};
