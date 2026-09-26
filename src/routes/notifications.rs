//! `notifications` routes (`NotificationsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/notifications.py` (v8.0.4). Every method except
//! [`NOTIFICATIONS_SET_PUSH_SETTINGS`] is network-scoped via `resolve_network_url` — a literal
//! suffix is appended to the network's own resolved base URL, and **no published `resources`
//! link name is ever consulted** for any of these paths (`resolve_network_url` only ever reads
//! the network's own `self_url`, never a link lookup by name). `link: None` on every constant
//! below reflects that: these are documentation/verb/version anchors for the no-parent path;
//! [`crate::endpoints::notifications::NotificationsApi`]'s private `resolve` helper implements
//! the actual `self_url`-preferred-else-template resolution `resolve_network_url` +
//! literal-suffix-append requires, which [`super::Resource::resolve`]'s own (named-link) parent
//! handling does not — see that module's docs.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/notifications` — the network's notification settings.
/// Live-verified.
///
/// Ported from `eero-api src/eero/api/notifications.py:36-64` (`NotificationsAPI.get_settings`).
pub const NOTIFICATIONS_GET_SETTINGS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/notifications",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/notifications` — set the network's notification settings.
///
/// Ported from `eero-api src/eero/api/notifications.py:66-111` (`NotificationsAPI.set_settings`).
pub const NOTIFICATIONS_SET_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/notifications",
    link: None,
};

/// `GET /2.2/networks/{network_id}/notifications/has_unread` — whether the network has unread
/// notifications. Live-verified.
///
/// Ported from `eero-api src/eero/api/notifications.py:113-141` (`NotificationsAPI.has_unread`).
pub const NOTIFICATIONS_HAS_UNREAD: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/notifications/has_unread",
    link: None,
};

/// `POST /2.2/networks/{network_id}/notifications/mark_read` — mark the network's notifications
/// as read.
///
/// Ported from `eero-api src/eero/api/notifications.py:142-178` (`NotificationsAPI.mark_read`).
/// Sends the literal two-character body `""` (`RequestBody::EmptyJsonString`), not `json: {}`.
pub const NOTIFICATIONS_MARK_READ: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/notifications/mark_read",
    link: None,
};

/// `GET /2.2/networks/{network_id}/notifications_history` — the network's notification history.
/// Live-verified. Note the underscore, not a slash, before `history` — verbatim from Python's own
/// f-string suffix.
///
/// Ported from `eero-api src/eero/api/notifications.py:180-217` (`NotificationsAPI.get_history`).
pub const NOTIFICATIONS_GET_HISTORY: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/notifications_history",
    link: None,
};

/// `PUT /2.2/account/push_settings` — the account's push notification settings. The one method in
/// this module that is not network-scoped: no `{id}` placeholder, `id_or_url` is ignored entirely
/// (`super::Resource::resolve`'s "fixed path" branch).
///
/// Ported from `eero-api src/eero/api/notifications.py:219-249`
/// (`NotificationsAPI.set_push_settings`).
pub const NOTIFICATIONS_SET_PUSH_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "account/push_settings",
    link: None,
};
