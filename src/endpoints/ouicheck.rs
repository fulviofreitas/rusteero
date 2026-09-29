//! OUI Check API: `eero-api`'s `OUICheckAPI` (`src/eero/api/ouicheck.py` at v8.0.4).
//!
//! Implements the one method this domain has left at v8.0.4: [`OUICheckApi::get_ouicheck`].
//! `OUICheckAPI.run_ouicheck` (the `v6.2.0` method this crate used to port as `run_ouicheck`) has
//! no v8.0.4 equivalent at all — `wiki/Migration.md:351` documents it as a removed operation the
//! API never actually had a matching endpoint for — and is **not** ported; see `PARITY.md`.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes::ApiVersion;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `OUICheckAPI` (`src/eero/api/ouicheck.py`).
///
/// Build one with [`OUICheckApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `OUICheckApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct OUICheckApi {
    transport: Arc<Transport>,
}

impl OUICheckApi {
    /// Wraps `transport` as an `OUICheckApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets OUI (vendor MAC prefix) check results for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/ouicheck.py:38-78` (`OUICheckAPI.get_ouicheck`). The
    /// URL is `{resolve_network_url(network_id, parent)}/ouicheck` — the same self-url-preferred
    /// resolution [`crate::params::resolve_network_url`] gives every other network-scoped
    /// method, plus a literal `"/ouicheck"` suffix; not a shape [`crate::routes::Resource`] can
    /// express (see [`crate::routes::ouicheck::OUICHECK_GET_OUICHECK`]'s own docs), so the URL is
    /// built by hand here.
    /// `serial` and `version` are sent as required query parameters (`?serial=..&version=..`);
    /// the API 404s without both.
    ///
    /// `serial`/`version` are `&str` rather than `Option<&str>`: Python's keyword-only
    /// `serial`/`version` arguments have no default, so a caller that omits either gets a
    /// `TypeError` at the call site — this port's non-`Option` parameters give the same
    /// compile-time guarantee `TypeError` gives at runtime, so no separate "missing" validation
    /// branch exists (unlike the empty-string check below, both parameters are always present by
    /// construction).
    ///
    /// **Validation runs before the URL is resolved**, matching `ouicheck.py:71-77` exactly
    /// (Python checks `serial`/`version` before it even reads the auth token — the one method in
    /// this crate's four G2 domains where validation precedes the authentication check other
    /// methods perform first via [`Transport::request`]'s own precondition).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "serial" | "version", .. }` if either value is empty.
    /// Otherwise returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_ouicheck(
        &self,
        network_id: &str,
        serial: &str,
        version: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if serial.is_empty() {
            return Err(Error::validation("serial", "must be a non-empty string"));
        }
        if version.is_empty() {
            return Err(Error::validation("version", "must be a non-empty string"));
        }

        let network_url = crate::params::resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            ApiVersion::V2_2,
        )?;
        let url = url::Url::parse(&format!(
            "{}/ouicheck",
            network_url.as_str().trim_end_matches('/')
        ))
        .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))?;

        self.transport
            .request(
                reqwest::Method::GET,
                url,
                &[
                    ("serial", serial.to_owned()),
                    ("version", version.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }
}
