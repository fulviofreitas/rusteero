//! Profile routes (`ProfilesAPI`) at v8.0.4.
//!
//! Ported from `eero-api src/eero/api/profiles.py` (v8.0.4). Every wire endpoint below uses the
//! v8 [`Resource`]/[`Nested`] model (`crate::routes::mod`'s module docs), resolved through
//! [`crate::links`]/[`crate::params`] so id-or-URL polymorphism and parent-link preference are
//! shared code, not hand-rolled per method.
//!
//! `GET_PROFILE`/`PAUSE_PROFILE`/`SET_PROFILE_DEVICES`/`RENAME_PROFILE`/`DELETE_PROFILE` all
//! share the same `networks/{network}/profiles/{profile}` resource family (`prefix: "profiles"`,
//! `suffix: ""`), one [`Nested`] constant per HTTP verb — mirroring how the pre-v8 `routes.rs`
//! declared one `Route` alias per verb for the same path. `link: None` on every one of them is
//! deliberate: none of these five is resolved via a *named* link lookup
//! (`crate::links::resolve_link`/`sub_resource_url`) the way [`GET_PROFILES`]/[`CREATE_PROFILE`]
//! are — `ProfilesAPI.get_profile`/`_update_profile` (`profiles.py:86-154`) instead prefer
//! [`crate::links::self_url`] on the caller-supplied `parent` (the *profile's own* cached
//! envelope, not the network's), which is a different preference rule than [`Nested::resolve`]
//! implements. [`crate::endpoints::profiles::ProfilesApi`]'s private `profile_url` helper
//! resolves that preference explicitly before falling back to `.resolve(host, network, profile,
//! None)` on these constants — see that module's docs for the full rationale.
//!
//! Removed since v6.2.0/pre-v8 (`eero-api` `tests/api/test_profiles.py::
//! TestProfilesAPIRemovedContentFilterSurface`): `update_profile_content_filter`,
//! `update_profile_block_list`, `get_blocked_applications`, `set_blocked_applications` — every
//! one PUT/GOT a field a profile does not have (silent no-op, live-verified). Their legacy
//! `Route` aliases (`UPDATE_PROFILE_CONTENT_FILTER`, `UPDATE_PROFILE_BLOCK_LIST`,
//! `GET_BLOCKED_APPLICATIONS`, `SET_BLOCKED_APPLICATIONS`) are deleted along with the endpoint
//! methods that used them; replaced by `crate::routes::dns_policies`.

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/profiles` — list profiles on a network.
///
/// Prefers the parent network envelope's own `profiles` link when supplied.
///
/// Ported from `eero-api src/eero/api/profiles.py:53-80` (`ProfilesAPI.get_profiles`).
pub const GET_PROFILES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/profiles",
    link: Some("profiles"),
};

/// `POST /2.2/networks/{network_id}/profiles` — create a profile.
///
/// Prefers the parent network envelope's own `profiles` link when supplied.
///
/// Ported from `eero-api src/eero/api/profiles.py:247-300` (`ProfilesAPI.create_profile`).
pub const CREATE_PROFILE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/profiles",
    link: Some("profiles"),
};

/// `GET /2.2/networks/{network_id}/profiles/{profile_id}` — a single profile's full object.
///
/// See the module docs for why `link` is `None` here: parent preference is resolved via
/// [`crate::links::self_url`] at the call site, not via this constant's (absent) named link.
///
/// Ported from `eero-api src/eero/api/profiles.py:86-118` (`ProfilesAPI.get_profile`).
pub const GET_PROFILE: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/profiles/{profile_id}` — pause/unpause a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:157-181` (`ProfilesAPI.pause_profile`, via
/// `_update_profile`). See [`GET_PROFILE`]'s docs for the `link: None` rationale.
pub const PAUSE_PROFILE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/profiles/{profile_id}` — replace a profile's device list.
///
/// Ported from `eero-api src/eero/api/profiles.py:208-245` (`ProfilesAPI.set_profile_devices`,
/// via `_update_profile`). See [`GET_PROFILE`]'s docs for the `link: None` rationale.
pub const SET_PROFILE_DEVICES: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/profiles/{profile_id}` — rename a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:304-330` (`ProfilesAPI.rename_profile`, via
/// `_update_profile`). See [`GET_PROFILE`]'s docs for the `link: None` rationale.
pub const RENAME_PROFILE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "",
    link: None,
};

/// `DELETE /2.2/networks/{network_id}/profiles/{profile_id}` — delete a profile.
///
/// Unlike its four siblings above, `ProfilesAPI.delete_profile` takes no `parent` kwarg at all
/// (`profiles.py:330-355`) — there is nothing to prefer over the template, ever.
///
/// Ported from `eero-api src/eero/api/profiles.py:330-355` (`ProfilesAPI.delete_profile`).
pub const DELETE_PROFILE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "",
    link: None,
};
