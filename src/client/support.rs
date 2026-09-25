//! `Client` methods for the `SupportAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets support information — returns the raw Eero API response.
    ///
    /// Ported from `get_support()` (`client.py:869-872`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_support(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.support().get_support(&network_id).await
    }

    // ==================== Insights, Support, Blacklist, Burst Reporters, OUICheck ====================
    //
    // None of these five has a `client.py` counterpart — `eero-api` never wrapped
    // `InsightsAPI.run_insights` (`api/insights.py:96`), `SupportAPI.request_support`
    // (`api/support.py:61`), `BlacklistAPI.add_to_blacklist`/`remove_from_blacklist`
    // (`api/blacklist.py:56,81`), `BurstReportersAPI.create_burst_reporter`
    // (`api/burst_reporters.py:56`) or `OUICheckAPI.run_ouicheck` (`api/ouicheck.py:56`) on
    // `EeroClient`, confirmed absent by grepping the committed `client.py` for each name — see
    // this file's own module docs for why they are added here anyway (item 1 of this phase's
    // task: every mutating endpoint method under `src/endpoints/` gets a `Client` wrapper, with
    // or without a `client.py` precedent). `auto_discover = false`, matching every sibling `GET`
    // in this same region of `client.py` (`get_insights`, `get_support`, `get_blacklist`,
    // `get_burst_reporters`, `get_ouicheck`, all between the `client.py:809` boundary and
    // `get_premium_status`). None of these five domains has a cache bucket at all (behaviour
    // brief §2.1), so every method below invalidates nothing — the same "no bucket to keep
    // consistent" reasoning as [`Client::create_reservation`]/[`Client::create_forward`] above,
    // not a judgement call specific to any one of them.

    /// Submits a support request for the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. `request_data` is
    /// forwarded verbatim as the request body, exactly like
    /// [`crate::endpoints::SupportApi::request_support`] itself. Invalidates nothing.
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
        self.api
            .support()
            .request_support(&network_id, request_data)
            .await
    }
}
