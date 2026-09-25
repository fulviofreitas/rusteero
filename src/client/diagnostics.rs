//! `Client` methods for the `DiagnosticsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `get_diagnostics()` (`client.py:809-812`). Resolves `network_id` with
    /// `auto_discover = false`, like every method in this section (`client.py:809` through
    /// `client.py:1350` — see the module docs' "G5" note for the two exceptions at the very end
    /// of the file).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted here. Otherwise, whatever status-mapped [`Error`] the
    /// request produces.
    pub async fn get_diagnostics(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.diagnostics().get_diagnostics(&network_id).await
    }

    // ==================== Diagnostics & Settings (mutations) ====================

    /// Runs network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `run_diagnostics` (`eero-api src/eero/client.py:814-817`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Invalidates nothing: `diagnostics` has no cache
    /// bucket at all (behaviour brief §2.1), and Python's own method body has no
    /// `del self._cache[...]` call to reproduce.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_diagnostics(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.diagnostics().run_diagnostics(&network_id).await
    }
}
