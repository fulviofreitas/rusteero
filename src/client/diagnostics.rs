//! `Client` methods for the `DiagnosticsAPI` domain (`eero-api src/eero/client.py`, diagnostics
//! section).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `get_diagnostics()` (`eero-api src/eero/client.py:1246-1251`). `auto_discover
    /// = false`, like every method in this section (`client.py:809` through `client.py:1350` —
    /// see the module docs' "G5" note for the two exceptions at the very end of the file). Passes
    /// the cached network envelope (if any) as `parent=`, so a fresh `resources.diagnostics` link
    /// wins over the hand-built template.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted here. Otherwise, whatever status-mapped [`Error`] the
    /// request produces.
    pub async fn get_diagnostics(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .diagnostics()
            .get_diagnostics(&network_id, parent.as_ref())
            .await
    }

    // ==================== Diagnostics (mutations) ====================

    /// Runs network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `run_diagnostics()` (`eero-api src/eero/client.py:1253-1273`). `auto_discover
    /// = false` — see [`Client::get_diagnostics`]. Invalidates nothing: `diagnostics` has no
    /// cache bucket at all (behaviour brief §2.1). Passes the cached network envelope (if any) as
    /// `parent=`. `device`/`symptom` are new since v8.0.0 — omitted keys are never sent, matching
    /// [`crate::endpoints::DiagnosticsApi::run_diagnostics`]. **Unverified body shape against a
    /// live network.**
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_diagnostics(
        &self,
        network_id: Option<&str>,
        device: Option<&str>,
        symptom: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .diagnostics()
            .run_diagnostics(&network_id, device, symptom, parent.as_ref())
            .await
    }
}
