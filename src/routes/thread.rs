//! Thread (smart-home mesh) routes (`ThreadAPI`), v8.4.1.
//!
//! Ported from `eero-api src/eero/api/thread.py` at v8.4.1. Per that module's own docstring
//! (`thread.py:6-11`): the read goes through the network's published `thread` link, like every
//! other sub-resource ([`GET_THREAD`], `link: Some("thread")`); every write targets the
//! **literal** `networks/{id}/thread` path directly, never through a published link — `parent`
//! is accepted by every Python write method for signature consistency with the rest of this
//! family, but is documented "Unused" and never consulted (`thread.py:89-90`). [`PUT_THREAD`]/
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
/// Ported from `eero-api src/eero/api/thread.py:40-68` (`ThreadAPI.get_thread`).
pub const GET_THREAD: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: Some("thread"),
};

/// `PUT /2.2/networks/{id}/thread` — the literal Thread path, used by `update_thread` and by
/// `set_thread_enabled`, which is an alias of it that sends `{"thread_enable": bool}`. `parent` is
/// never consulted — see the module docs.
///
/// Ported from `eero-api src/eero/api/thread.py:101-155` (`ThreadAPI.update_thread`), whose
/// `resource_url(network_id, "networks/{id}/thread")` call (`thread.py:153`) `set_thread_enabled`
/// (`thread.py:70-99`) reaches through the alias.
pub const PUT_THREAD: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: None,
};

/// `POST /2.2/networks/{id}/thread` — the literal Thread path, for credential regeneration.
/// `parent` is never consulted — see the module docs.
///
/// Ported from `eero-api src/eero/api/thread.py:157-191`
/// (`ThreadAPI.regenerate_thread_credentials`).
pub const POST_THREAD: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/thread",
    link: None,
};
