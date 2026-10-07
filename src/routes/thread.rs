//! Thread (smart-home mesh) routes (`ThreadAPI`), v8.0.4.
//!
//! Ported from `eero-api src/eero/api/thread.py` at v8.0.4. Per that module's own docstring
//! (`thread.py:6-11`): the read goes through the network's published `thread` link, like every
//! other sub-resource ([`GET_THREAD`], `link: Some("thread")`); every write targets the
//! **literal** `networks/{id}/thread` path directly, never through a published link — `parent`
//! is accepted by every Python write method for signature consistency with the rest of this
//! family, but is documented "Unused" and never consulted (`thread.py:88-89`). [`PUT_THREAD`]/
//! [`POST_THREAD`] below model that with `link: None`: [`Resource::resolve`] falls straight to
//! [`crate::links::resource_url`] whenever `link` is `None`, ignoring whatever `parent` a caller
//! passes — the exact "accepted but inert" behaviour the Python source calls out. A port must
//! resist "fixing" this into a `link`-preferring route for consistency with the rest of the
//! crate; the asymmetry is upstream-declared, not an oversight.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/thread` — Thread (smart-home mesh) status, preferring the network's
/// published `thread` link when a `parent` envelope carries one.
///
/// Ported from `eero-api src/eero/api/thread.py:40-66` (`ThreadAPI.get_thread`).
pub const GET_THREAD: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: Some("thread"),
};

/// `PUT /2.2/networks/{id}/thread` — the literal Thread path, shared by `set_thread_enabled` and
/// `update_thread`. `parent` is never consulted — see the module docs.
///
/// Ported from `eero-api src/eero/api/thread.py:70-103` (`ThreadAPI.set_thread_enabled`) and
/// `thread.py:105-159` (`ThreadAPI.update_thread`), which resolve via the identical
/// `resource_url(network_id, "networks/{id}/thread")` call.
pub const PUT_THREAD: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: None,
};

/// `POST /2.2/networks/{id}/thread` — the literal Thread path, for credential regeneration.
/// `parent` is never consulted — see the module docs.
///
/// Ported from `eero-api src/eero/api/thread.py:161-186`
/// (`ThreadAPI.regenerate_thread_credentials`).
pub const POST_THREAD: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: None,
};
