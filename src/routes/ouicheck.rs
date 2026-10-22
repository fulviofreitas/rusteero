//! OUI-check routes (`OUICheckAPI`), ported from `eero-api src/eero/api/ouicheck.py` at v8.0.4.
//!
//! `get_ouicheck` resolves its URL the same self-url-preferred way as several `eeros.rs` methods
//! (`crate::params::resolve_network_url`, not a named `resources` link) plus a literal
//! `"/ouicheck"` suffix — not a shape [`crate::routes::Resource::resolve`] can express on its
//! own (the suffix is appended *after* self-url preference is resolved, not baked into a
//! `template`), so [`OUICHECK_GET_OUICHECK`] below is a **documentation-only** marker, exactly
//! like [`crate::routes::permissions::PERMISSIONS_GET_PERMISSIONS`]: `src/endpoints/ouicheck.rs` builds the
//! URL by hand from [`crate::params::resolve_network_url`] plus the literal suffix, never via
//! [`crate::routes::Resource::resolve`] on this constant.
//! `OUICheckAPI.run_ouicheck` (the `v6.2.0` route this file used to declare as `RUN_OUICHECK`)
//! has no v8.0.4 equivalent — the API has no such operation (`wiki/Migration.md:351`) — and is
//! not ported.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// Documentation-only route marker for `networks/{id}/ouicheck` — see the module docs for why
/// [`OUICheckApi::get_ouicheck`](crate::endpoints::ouicheck::OUICheckApi::get_ouicheck) never
/// resolves through this constant via [`Resource::resolve`].
pub const OUICHECK_GET_OUICHECK: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/ouicheck",
    link: None,
};
