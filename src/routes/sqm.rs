//! Smart Queue Management routes (`SqmAPI`), v8.0.4.
//!
//! Ported from `eero-api src/eero/api/sqm.py` at v8.0.4. `GET_SQM_SETTINGS` is *not* modelled as
//! a [`Resource`], for the same reason `security.rs` documents at length:
//! `SqmAPI.get_sqm_settings` resolves via the module's own `_network_own_url` helper
//! (`sqm.py:24-29`, byte-for-byte identical to `security.py`'s), which prefers a supplied
//! parent's own `url` (`self_url`) — the exact rule [`crate::params::resolve_network_url`]
//! already implements. [`crate::endpoints::sqm::SqmApi::get_sqm_settings`] calls that helper
//! directly plus [`crate::transport::Transport::request`], rather than going through a `Resource`
//! constant that cannot express "prefer `self_url`" at all.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `PUT /2.2/networks/{id}/settings` — the network's settings sub-resource; `SqmAPI.set_sqm` is
/// its sole writer.
///
/// Ported from `eero-api src/eero/api/sqm.py:79-125` (`SqmAPI.set_sqm`). A separate constant from
/// [`crate::routes::dns::DNS_PUT_SETTINGS`]/[`crate::routes::security::SECURITY_PUT_SETTINGS`]
/// (same template/link, different domain module) — route constant names must be unique
/// crate-wide.
pub const SQM_PUT_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};
