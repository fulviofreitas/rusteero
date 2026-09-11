//! Network settings API: `eero-api`'s `SettingsAPI`, which has exactly one method.
//!
//! Ported from `eero-api src/eero/api/settings.py`. Unlike `dns`, `security` and `sqm`,
//! `SettingsAPI.get_settings` is not an alias of `GET_NETWORK` — it is the only Python method in
//! this batch that actually hits the dedicated `networks/{network_id}/settings` sub-resource
//! rather than the full network object.
//!
//! The single method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, this method
//! does not duplicate that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `SettingsAPI` (`src/eero/api/settings.py`) — a single read-only method.
///
/// Build one with [`SettingsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `SettingsApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct SettingsApi {
    transport: Arc<Transport>,
}

impl SettingsApi {
    /// Wraps `transport` as a `SettingsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the network-wide settings resource — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/settings.py:33-54` (`SettingsAPI.get_settings`).
    /// Sends `GET` [`crate::routes::GET_SETTINGS`], which — unlike
    /// [`crate::endpoints::dns::DnsApi::get_dns_settings`],
    /// [`crate::endpoints::security::SecurityApi::get_security_settings`] and
    /// [`crate::endpoints::sqm::SqmApi::get_sqm_settings`] — targets the dedicated
    /// `networks/{network_id}/settings` sub-resource, not the full network object; it is the
    /// `GET` counterpart of [`crate::routes::PUT_NETWORK_SETTINGS`], same path, opposite verb.
    /// This method never extracts, renames or reshapes any field — the caller reads whatever
    /// keys the server includes out of the returned envelope's `data`, exactly as it comes over
    /// the wire.
    pub async fn get_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_SETTINGS, &[("network_id", network_id)], None)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // `SettingsAPI` has no mutation methods of its own in `eero-api` (`settings.py` defines only
    // `get_settings`); the shared `PUT networks/{network_id}/settings` resource this method's GET
    // counterpart reads is instead written by setters spread across `NetworksAPI.set_network_name`
    // and the `DnsAPI`/`SecurityAPI`/`SqmAPI` phase-5 setters noted in their own files. Nothing to
    // reserve here.
    // ---------------------------------------------------------------------------------------
}
