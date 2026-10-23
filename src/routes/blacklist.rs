//! Device-blacklist routes (`BlacklistAPI`).

// ------------------------------ blacklist (`BlacklistAPI`) -----------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

// ============================================================================================
// v8.0.4 constants. Every `BlacklistApi` method in
// `src/endpoints/blacklist.rs` is built on one of these.
// ============================================================================================

/// `GET /2.2/networks/{id}/blacklist` — list blacklisted (blocked) devices, preferring the
/// network's own published `device_blacklist` link when a `parent` envelope is supplied.
///
/// Ported from `BlacklistAPI._blacklist_url` (`eero-api src/eero/api/blacklist.py:53-70` at
/// `v8.0.4`): `sub_resource_url(network, "networks/{id}/blacklist",
/// link=DEVICE_BLACKLIST_LINK, parent=as_envelope(parent), version=API_VERSION_DEFAULT)`, where
/// `DEVICE_BLACKLIST_LINK = "device_blacklist"` (`blacklist.py:32`). Used by
/// `BlacklistApi::get_blacklist`, `BlacklistApi::add_to_blacklist` (different HTTP method — see
/// [`V8_ADD_TO_BLACKLIST`]) and `BlacklistApi::remove_from_blacklist` (as the base this route's
/// `resolve` result is fed through [`crate::links::child_url`]). API-Reference.md:851, read.
pub const V8_GET_BLACKLIST: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/blacklist",
    link: Some("device_blacklist"),
};

/// `POST /2.2/networks/{id}/blacklist` — add a device (by MAC) to the blacklist, via a
/// **form-encoded** `mac` field (not JSON).
///
/// Ported from `BlacklistAPI.add_to_blacklist` (`blacklist.py:98-135` at `v8.4.1`): same URL
/// resolution as [`V8_GET_BLACKLIST`], `data={"mac": mac}` (`blacklist.py:135`). A verified
/// write (2026-10-06, form-encoded `mac`): the endpoint logs no uncharacterised-write warning.
pub const V8_ADD_TO_BLACKLIST: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/blacklist",
    link: Some("device_blacklist"),
};

// `BlacklistAPI.remove_from_blacklist` (`blacklist.py:139-166`) has no route constant of its
// own: it resolves [`V8_GET_BLACKLIST`] for the collection URL, then appends `mac_or_device_id`
// via `child_url` (`links.py:280-298`), which validates it as a single path-segment identifier
// *before* any request is sent (`_validate_identifier` — `links.py:47-76`) — see
// `BlacklistApi::remove_from_blacklist`'s own docs.
