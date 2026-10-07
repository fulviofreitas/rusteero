//! `Client` methods for the `BlacklistAPI` domain.
//!
//! # `add_to_blacklist`/`remove_from_blacklist`: removed
//!
//! The pre-8.0.4 port of this crate added `Client::add_to_blacklist`/
//! `Client::remove_from_blacklist` with an explicit "no `client.py` precedent" note. At v8.0.4,
//! `EeroClient` still has no such wrappers — device blocking moved to
//! [`Client::block_device`]/`Client::unblock_device`] (`crate::client::devices`), which invoke
//! the exact same underlying [`crate::endpoints::blacklist::BlacklistApi::add_to_blacklist`]/
//! [`crate::endpoints::blacklist::BlacklistApi::remove_from_blacklist`] domain methods and
//! already invalidate the `devices` cache bucket. These two `Client` methods are removed; the
//! domain methods themselves remain reachable via `client.api().blacklist()`, matching how
//! `EeroClient` exposes domain-API-only access for methods it never wraps.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the device blacklist — returns the raw Eero API response.
    ///
    /// Ported from `get_blacklist()` (`eero-api src/eero/client.py:1489-1495` at `v8.0.4`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the cached network
    /// envelope as `parent=` (`+net`, `_network_parent_kwargs`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blacklist(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .blacklist()
            .get_blacklist(&network_id, parent.as_ref())
            .await
    }
}
