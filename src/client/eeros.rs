//! `Client` methods for the `EerosAPI` domain (`eero-api src/eero/client.py`, eeros section).
//!
//! See `.claude/tasks/briefs/v8/g2-eeros.md` §3 for the full "facade name/shape divergences"
//! list this file reproduces: which methods pass `_eero_parent_kwargs`/`_network_parent_kwargs`,
//! which invalidate `eeros[{nid}_eeros]`, and the two "resolves `network_id` only for cache
//! invalidation, never forwards it to the domain call" quirks ([`Client::port_action`],
//! [`Client::nightlight_override`]).

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_eeros()` (`eero-api src/eero/client.py:565-592`). `auto_discover = true`
    /// — see [`Client::get_network`]. On a cache miss, passes the cached network envelope (if
    /// any) as `parent=`, so a fresh `resources.eeros` link wins over the hand-built template.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_eeros(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::eeros(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .eeros()
            .get_eeros(&network_id, parent.as_ref())
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_eero()` (`eero-api src/eero/client.py:594-616`). `auto_discover = true` —
    /// see [`Client::get_network`]. **Never cached**: `refresh_cache` is accepted for signature
    /// parity with Python but intentionally never consulted (`wiki/API-Reference.md:144`) — this
    /// is always a live call. Passes the cached eero entry (if any) as `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_eero(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let _ = refresh_cache;
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        self.api.eeros().get_eero(eero_id, parent.as_ref()).await
    }

    /// Reboots a single Eero device — returns the raw Eero API response.
    ///
    /// Ported from `reboot_eero()` (`eero-api src/eero/client.py:618-643`). `auto_discover = true`
    /// — see [`Client::get_network`]. On success, invalidates `eeros[{nid}_eeros]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn reboot_eero(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .reboot_eero(eero_id, parent.as_ref())
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets an Eero device's Wi-Fi location label — returns the raw Eero API response.
    ///
    /// Ported from `set_location()` (`eero-api src/eero/client.py:645-661`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. On success, invalidates `eeros[{nid}_eeros]`.
    /// **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_location(
        &self,
        eero_id: &str,
        location: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .set_location(eero_id, location, parent.as_ref())
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Gets a single Eero device's client connections — returns the raw Eero API response.
    ///
    /// Ported from `get_connections()` (`eero-api src/eero/client.py:663-673`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Not cached (`connections` has no cache bucket).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_connections(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        self.api
            .eeros()
            .get_connections(eero_id, parent.as_ref())
            .await
    }

    // ==================== LED & Nightlight ====================

    /// Gets LED status for an Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_led_status()` (`eero-api src/eero/client.py:1854-1861`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Not cached.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_led_status(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        self.api
            .eeros()
            .get_led_status(eero_id, parent.as_ref())
            .await
    }

    /// Turns an Eero device's status LED on or off — returns the raw Eero API response.
    ///
    /// Ported from `set_led()` (`eero-api src/eero/client.py:1863-1872`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. On success, invalidates `eeros[{nid}_eeros]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_led(
        &self,
        eero_id: &str,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .set_led(eero_id, enabled, parent.as_ref())
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets an Eero device's status LED brightness — returns the raw Eero API response.
    ///
    /// Ported from `set_led_brightness()` (`eero-api src/eero/client.py:1874-1883`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. On success, invalidates
    /// `eeros[{nid}_eeros]`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "brightness", .. }` if `brightness` is outside
    /// `0..=100` (v8.0.4 rejects rather than clamping — see
    /// [`crate::endpoints::EerosApi::set_led_brightness`]). Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_led_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .set_led_brightness(eero_id, brightness, parent.as_ref())
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `get_nightlight()` (`eero-api src/eero/client.py:1885-1892`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Not cached.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FeatureUnavailable`] if the eero has no nightlight. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn get_nightlight(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        self.api
            .eeros()
            .get_nightlight(eero_id, parent.as_ref())
            .await
    }

    /// Sets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `set_nightlight()` (`eero-api src/eero/client.py:1894-1920`). Every setting is
    /// optional and independent, delegating unchanged to
    /// [`crate::endpoints::EerosApi::set_nightlight`] — see that method's own docs for the
    /// brightness validation, the "at least one field" rule, and the discovery/URL-resolution
    /// order. `auto_discover = false` — see [`Client::get_diagnostics`]. On success, invalidates
    /// `eeros[{nid}_eeros]`. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::EerosApi::set_nightlight`], and [`Client::get_diagnostics`]. The
    /// cache is left untouched on any `Err`.
    pub async fn set_nightlight(
        &self,
        eero_id: &str,
        enabled: Option<bool>,
        brightness_percentage: Option<i32>,
        schedule: Option<Value>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .set_nightlight(
                eero_id,
                enabled,
                brightness_percentage,
                schedule,
                parent.as_ref(),
            )
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets only the nightlight brightness for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_nightlight_brightness()` (`eero-api src/eero/client.py:1922-1935`); a
    /// pure delegator to [`Client::set_nightlight`], sharing its cache invalidation and `parent=`
    /// resolution exactly — matches `eeros.py:551-579`'s own delegation shape one layer up.
    ///
    /// # Errors
    ///
    /// See [`Client::set_nightlight`]. Never returns the "at least one field" validation error: a
    /// `brightness_percentage` value is always supplied here.
    pub async fn set_nightlight_brightness(
        &self,
        eero_id: &str,
        brightness_percentage: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, Some(brightness_percentage), None, network_id)
            .await
    }

    /// Sets only the nightlight schedule for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_nightlight_schedule()` (`eero-api src/eero/client.py:1937-1950`); a pure
    /// delegator to [`Client::set_nightlight`], sharing its cache invalidation and `parent=`
    /// resolution exactly.
    ///
    /// # Errors
    ///
    /// See [`Client::set_nightlight`]. Never returns the "at least one field" validation error:
    /// `schedule` is always supplied here.
    pub async fn set_nightlight_schedule(
        &self,
        eero_id: &str,
        schedule: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, None, Some(schedule), network_id)
            .await
    }

    /// Performs a node-level action on an Eero device — returns the raw Eero API response.
    ///
    /// Ported from `node_action()` (`eero-api src/eero/client.py:3074-3087`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. On success, invalidates `eeros[{nid}_eeros]`.
    /// **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::EerosApi::node_action`], and [`Client::get_diagnostics`]. The
    /// cache is left untouched on any `Err`.
    pub async fn node_action(
        &self,
        eero_id: &str,
        action: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.eero_parent(&network_id, eero_id);
        let response = self
            .api
            .eeros()
            .node_action(eero_id, action, parent.as_ref())
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Performs a port-level action on an Eero device's interface — returns the raw Eero API
    /// response.
    ///
    /// Ported from `port_action()` (`eero-api src/eero/client.py:3088-3096`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. **Quirk, reproduced deliberately**: `network_id`
    /// is resolved only to invalidate `eeros[{nid}_eeros]` on success — it is never forwarded to
    /// [`crate::endpoints::EerosApi::port_action`], which has no `network_id` parameter at all
    /// (see that method's own docs).
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::EerosApi::port_action`], and [`Client::get_diagnostics`]. The
    /// cache is left untouched on any `Err`.
    pub async fn port_action(
        &self,
        eero_id: &str,
        interface_number: u32,
        action: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .port_action(eero_id, interface_number, action)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Cycles an Eero device's status LED through a colour sequence — returns the raw Eero API
    /// response.
    ///
    /// Ported from `led_cycle()` (`eero-api src/eero/client.py:3097-3104`). No `network_id`
    /// parameter at all, matching [`crate::endpoints::EerosApi::led_cycle`] — addressed by
    /// `eero_serial` alone, so nothing is invalidated (there is no network context to key a
    /// cache invalidation on). **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::EerosApi::led_cycle`].
    pub async fn led_cycle(
        &self,
        eero_serial: &str,
        colors: &[String],
        duration: u32,
        time_per_color: u32,
    ) -> Result<Envelope, Error> {
        self.api
            .eeros()
            .led_cycle(eero_serial, colors, duration, time_per_color)
            .await
    }

    /// Previews a nightlight brightness value without persisting it — returns the raw Eero API
    /// response.
    ///
    /// Ported from `nightlight_override()` (`eero-api src/eero/client.py:3105-3115`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. **Quirk, reproduced
    /// deliberately**: `network_id` is resolved only to invalidate `eeros[{nid}_eeros]` on
    /// success — it is never forwarded to
    /// [`crate::endpoints::EerosApi::nightlight_override`], which has no `network_id` parameter
    /// at all (see that method's own docs).
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::EerosApi::nightlight_override`], and [`Client::get_diagnostics`].
    /// The cache is left untouched on any `Err`.
    pub async fn nightlight_override(
        &self,
        eero_id: &str,
        brightness_percentage: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .nightlight_override(eero_id, brightness_percentage)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Gets an Eero device's support/diagnostics summary — returns the raw Eero API response.
    ///
    /// Ported from `get_eero_support()` (`eero-api src/eero/client.py:3116-3122`). No
    /// `network_id` parameter at all, matching
    /// [`crate::endpoints::EerosApi::get_eero_support`] — addressed by `eero_serial` alone. Not
    /// cached.
    pub async fn get_eero_support(&self, eero_serial: &str) -> Result<Envelope, Error> {
        self.api.eeros().get_eero_support(eero_serial).await
    }
}
