//! `Client` methods for the `SupportAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

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
    // `EeroClient`, confirmed absent by grepping the committed `client.py` for each name.
    //
    // **`request_support` removed** (phase-G fix list item 6): an earlier phase of this port
    // added a `Client::request_support` wrapper anyway, with no Python precedent to cite — removed
    // to match `client.py` exactly. [`crate::endpoints::support::SupportApi::request_support`]
    // itself is unaffected and still callable directly through [`crate::api::EeroApi`].
}
