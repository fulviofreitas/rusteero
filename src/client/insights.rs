//! `Client` methods for the `InsightsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Queries insights time-series data — returns the raw Eero API response.
    ///
    /// Ported from `get_insights()` (`client.py:824-857`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Unlike Python, `cadence` has no SDK-supplied default: the
    /// underlying [`crate::endpoints::InsightsApi::get_insights`] already made that call (see its
    /// own docs) and this method inherits it.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_insights(
        &self,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        insight_type: &str,
        cadence: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_insights(&network_id, start, end, insight_type, cadence)
            .await
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

    /// Runs an on-demand insights computation — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_insights(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.insights().run_insights(&network_id).await
    }
}
