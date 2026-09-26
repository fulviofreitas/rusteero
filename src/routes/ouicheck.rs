//! OUI-check routes (`OUICheckAPI`), ported from `eero-api src/eero/api/ouicheck.py` at v8.0.4.
//!
//! `get_ouicheck` resolves its URL the same self-url-preferred way as several `eeros.rs` methods
//! (`crate::params::resolve_network_url`, not a named `resources` link) plus a literal
//! `"/ouicheck"` suffix — not a shape [`crate::routes::Resource::resolve`] can express on its
//! own, so no `Resource` constant is declared here; `src/endpoints/ouicheck.rs` builds the URL by
//! hand from [`crate::params::resolve_network_url`] instead. `OUICheckAPI.run_ouicheck` (the
//! `v6.2.0` route this file used to declare as `RUN_OUICHECK`) has no v8.0.4 equivalent — the API
//! has no such operation (`wiki/Migration.md:351`) — and is not ported.
