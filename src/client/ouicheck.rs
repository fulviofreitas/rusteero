//! `Client` methods for the `OUICheckAPI` domain (`eero-api src/eero/client.py`, ouicheck
//! section).
//!
//! `OUICheckAPI.run_ouicheck` has no v8.0.4 equivalent (see
//! `src/endpoints/ouicheck.rs`'s module docs) — the old, `client.py`-less
//! [`Client::run_ouicheck`] convenience this file used to expose is **removed** along with it;
//! see `PARITY.md`.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets OUI (vendor MAC prefix) check results for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_ouicheck()` (`eero-api src/eero/client.py:1807-1825`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. `serial`/`version` are required (not `Option`),
    /// matching Python's required keyword-only parameters (new since v7.0.0 — the pre-v8.0.4
    /// `Client::get_ouicheck` this port replaces took neither and always 404'd).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "serial" | "version", .. }` if either value is empty.
    /// Otherwise see [`Client::get_diagnostics`].
    pub async fn get_ouicheck(
        &self,
        serial: &str,
        version: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .ouicheck()
            .get_ouicheck(&network_id, serial, version, parent.as_ref())
            .await
    }
}
