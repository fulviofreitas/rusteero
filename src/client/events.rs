//! `Client` methods for the `EventsAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/events.py` (v8.0.4) by way of `EeroClient`'s own
//! `events`-scoped wrappers in `client.py`. Every wrapper here passes the cached network
//! envelope as `parent` (`+net`, `.claude/tasks/briefs/v8/client.md` §4's `events` rows), so a
//! fresher self-url on that cached envelope wins over the literal template — see
//! [`crate::endpoints::events::EventsApi`]'s own docs for the resolution this enables.

use super::Client;
use crate::endpoints::events::GetChannelUtilizationOptions;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets app-facing events — returns the raw Eero API response.
    ///
    /// Ported from `get_app_events()` (`client.py:2318-2333`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_app_events(
        &self,
        page_size: Option<u32>,
        timestamp: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .events()
            .get_app_events(&network_id, page_size, timestamp, parent.as_ref())
            .await
    }

    /// Gets network scan results — returns the raw Eero API response.
    ///
    /// Ported from `get_network_scan()` (`client.py:2334-2340`). `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_network_scan(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .events()
            .get_network_scan(&network_id, parent.as_ref())
            .await
    }

    /// Gets a channel-utilization report — returns the raw Eero API response.
    ///
    /// Ported from `get_channel_utilization()` (`client.py:2341-2370`). `auto_discover = false`;
    /// `network_id` is the sole positional argument on the Python side too.
    ///
    /// # Errors
    ///
    /// Returns whatever validation error
    /// [`crate::endpoints::events::EventsApi::get_channel_utilization`] returns for a bad
    /// `options.band`/`options.busy_threshold`/`options.granularity`. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn get_channel_utilization(
        &self,
        start: &str,
        end: &str,
        options: &GetChannelUtilizationOptions<'_>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .events()
            .get_channel_utilization(&network_id, start, end, options, parent.as_ref())
            .await
    }
}
