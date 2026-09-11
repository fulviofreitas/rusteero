//! DNS settings API: the read-only (`GET`) half of `eero-api`'s `DnsAPI`.
//!
//! Ported from `eero-api src/eero/api/dns.py`. This phase (3, GET-only) covers
//! `DnsAPI.get_dns_settings` only.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `DnsAPI` (`src/eero/api/dns.py`).
///
/// Build one with [`DnsApi::new`], wrapping a [`Transport`] already shared with the rest of the
/// (not-yet-built) `EeroApi` aggregator — `DnsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct DnsApi {
    transport: Arc<Transport>,
}

impl DnsApi {
    /// Wraps `transport` as a `DnsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets DNS configuration for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:36-57` (`DnsAPI.get_dns_settings`). Sends `GET`
    /// [`crate::routes::GET_DNS_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated DNS sub-resource; there is no such sub-resource
    /// on the wire. The caller is expected to read the relevant keys (`dns_caching`,
    /// `custom_dns`, `ipv6_upstream`, and any others the server includes) out of the returned
    /// envelope's `data`, exactly as Python's own docstring instructs. This method never
    /// extracts, renames or reshapes any field — doing so would transform the raw payload the
    /// rest of this crate promises never to touch.
    pub async fn get_dns_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_DNS_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): DnsAPI's mutation methods go here — `set_dns_caching`
    // (`dns.py:59-87`), `set_custom_dns` (`dns.py:89-124`), `clear_custom_dns` (`dns.py:126-135`,
    // delegates to `set_custom_dns([])`), `set_dns_mode` (`dns.py:137-184`) and `set_ipv6_dns`
    // (`dns.py:186-214`). Notes for whoever implements them:
    //
    // - All five PUT `networks/{network_id}/settings`
    //   (`crate::routes::PUT_NETWORK_SETTINGS`), the exact same wire resource `SecurityAPI` and
    //   `SqmAPI`'s setters also target — phase 5 should share one `put_network_settings(network_id,
    //   body)` helper on this resource rather than repeating the call five times across three
    //   files.
    // - `set_custom_dns` truncates the caller-supplied server list to at most 2 entries
    //   (`dns.py:114-116`), logging a warning when it does; the truncation must happen before the
    //   request body is built, not after.
    // - `set_dns_mode` rejects an unknown `mode` string. Python fabricates a local
    //   `{"meta": {"code": 400}, "data": {}}` response without ever calling the server
    //   (`dns.py:174-176`); this port should instead return `Error::Validation` for that case —
    //   inventing a fake envelope would be a worse violation of the raw-payload contract than
    //   simply refusing before any request is sent.
    // ---------------------------------------------------------------------------------------
}
