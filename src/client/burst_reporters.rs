//! `Client` methods for the `BurstReportersAPI` domain.
//!
//! [`Client::create_burst_reporter`] is the one wrapper (`client.py:1534-1581` at `v8.4.1`).
//! `get_burst_reporters` was removed upstream in v8.0.0 — the endpoint 404s — and has no wrapper.
//! The facade takes no `reporter_data`: it calls the domain method with no body, so the `""`
//! body applies, no deprecation notice is logged and no uncharacterised-write warning is
//! logged. A caller that still needs the deprecated JSON body reaches
//! `client.api().burst_reporters()` directly.

use serde_json::Value;

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Requests a live-usage burst for the network — returns the raw Eero API response.
    ///
    /// Ported from `create_burst_reporter()` (`eero-api src/eero/client.py:1534-1581` at
    /// `v8.4.1`). POSTs the `""` body to the network's `burst_reporters` link
    /// (`POST /2.2/networks/{id}/burst_reporters` by default). The response carries
    /// `data.next_burst` (a timestamp string). About 12 seconds later the device list's
    /// `usage.down_mbps` and `usage.up_mbps` carry fresh values; before the POST they are zero or
    /// stale. This does not run a speed test and changes no setting. At most one burst may be
    /// requested per minute per network.
    ///
    /// To read a network-wide throughput figure: request a burst, wait roughly 12 seconds, read
    /// the devices with `get_devices(.., true)` and sum `usage.down_mbps` / `usage.up_mbps`
    /// across the devices where `connected` is true.
    ///
    /// Live-verified 2026-10-20: the usage values refreshed within 12 seconds and had largely
    /// decayed again by 30 seconds, no WAN-level usage field appeared on the network envelope,
    /// and no node rebooted. Read the device list about 12 seconds after the POST, not later.
    /// Never retry it in a loop. No uncharacterised-write warning is logged.
    ///
    /// `auto_discover = false`. `parent` is the network's own envelope; when `None`, the cached
    /// network envelope is used if one is fresh (`_network_parent_kwargs_or`). Read only. On
    /// success, invalidates only `devices[{nid}_devices]` — single-device entries, other
    /// networks and the network cache survive — so a caller that polls faster than the 60 s
    /// cache TTL should still pass `refresh_cache = true` on the follow-up
    /// [`Client::get_devices`]. Not retried.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn create_burst_reporter(
        &self,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        let response = self
            .api
            .burst_reporters()
            .create_burst_reporter(&network_id, None, parent.as_ref())
            .await?;
        self.invalidate_device_cache(network_id.as_str(), &[]);
        Ok(response)
    }
}
