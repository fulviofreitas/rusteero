//! `Client` methods for the `SupportAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets support information — returns the raw Eero API response.
    ///
    /// Ported from `get_support()` (`client.py:1482-1486`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_support(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .support()
            .get_support(&network_id, parent.as_ref())
            .await
    }

    // ==================== Insights, Support, Blacklist, Burst Reporters, OUICheck ====================
    //
    // None of these five has a `client.py` counterpart — `eero-api` never wrapped
    // `InsightsAPI.run_insights` (`api/insights.py:96`), `SupportAPI.request_support`
    // (`api/support.py:65`), `BlacklistAPI.add_to_blacklist`/`remove_from_blacklist`
    // (`api/blacklist.py:56,81`), `BurstReportersAPI.create_burst_reporter`
    // (`api/burst_reporters.py:56`) or `OUICheckAPI.run_ouicheck` (`api/ouicheck.py:56`) on
    // `EeroClient`, confirmed absent by grepping the committed `client.py` for each name — see
    // this file's own module docs for why they are added here anyway. `auto_discover = false`,
    // matching every sibling `GET` in this same region of `client.py`. None of these five domains
    // has a cache bucket at all (behaviour brief §2.1), so every method below invalidates
    // nothing.

    /// Submits a support request for the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. `request_data` is
    /// forwarded verbatim as the request body, exactly like
    /// [`crate::endpoints::SupportApi::request_support`] itself. Passes the cached network
    /// envelope as `parent=`, for consistency with its read sibling
    /// [`Client::get_support`] — judgement call, no Python precedent to match or diverge from.
    /// Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn request_support(
        &self,
        request_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .support()
            .request_support(&network_id, request_data, parent.as_ref())
            .await
    }
}
