//! `Client` methods for the `EerosAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_eeros()` (`client.py:361-386`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
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
        let response = self.api.eeros().get_eeros(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    // ==================== LED & Nightlight ====================

    /// Gets LED status for an Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_led_status()` (`client.py:1032-1037`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. `network_id` is still resolved and validated even though it
    /// is never forwarded to the underlying request: [`crate::endpoints::EerosApi::get_led_status`]
    /// drops it entirely, since Python's own `network_id` parameter here is unused (see that
    /// method's own docs) — this method keeps resolving it anyway, purely for call-signature and
    /// validation parity with `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_led_status(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, false).await?;
        self.api.eeros().get_led_status(eero_id).await
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `get_nightlight()` (`client.py:1059-1064`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]; see [`Client::get_led_status`] for why `network_id` is
    /// resolved but not forwarded.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_nightlight(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, false).await?;
        self.api.eeros().get_nightlight(eero_id).await
    }

    // ==================== Eeros (uncached pass-through) ====================

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_eero()` (`client.py:388-408`). Unlike [`Client::get_eeros`] (the list),
    /// this single-item getter is **not** cached (brief §2.1/G9's asymmetry note is about
    /// `get_device_priority` specifically, but the same "list is cached, single item is not"
    /// shape applies here by construction — `get_eero` was never one of the eight cached
    /// getters in the first place). `auto_discover = true` (Python omits the argument, matching
    /// `get_network`/`get_eeros`/etc., since this method appears before the `client.py:809`
    /// section boundary). `network_id` is resolved and validated but never forwarded to the
    /// underlying request — see [`Client::get_led_status`] for why.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_eero(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, true).await?;
        self.api.eeros().get_eero(eero_id).await
    }

    // ==================== Eeros (mutations) ====================

    /// Reboots a single Eero device — returns the raw Eero API response.
    ///
    /// Ported from `reboot_eero` (`eero-api src/eero/client.py:410-432`). `auto_discover = true`
    /// (before the `client.py:809` boundary, matching [`Client::get_eero`] just above it in this
    /// file). `network_id` is resolved only to build the `eeros[{nid}_eeros]` cache key below —
    /// it is never forwarded to [`crate::endpoints::EerosApi::reboot_eero`], which drops it for
    /// the same reason [`Client::get_led_status`] does (see that method's docs). On success,
    /// invalidates `eeros[{nid}_eeros]` (`client.py:427-430`).
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
        let response = self.api.eeros().reboot_eero(eero_id).await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Turns an Eero device's status LED on or off — returns the raw Eero API response.
    ///
    /// Ported from `set_led` (`eero-api src/eero/client.py:1039-1050`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded; see
    /// [`Client::reboot_eero`]. On success, invalidates `eeros[{nid}_eeros]`
    /// (`client.py:1046-1048`).
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
        let response = self.api.eeros().set_led(eero_id, enabled).await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets an Eero device's status LED brightness — returns the raw Eero API response.
    ///
    /// Ported from `set_led_brightness` (`eero-api src/eero/client.py:1052-1057`). `auto_discover
    /// = false` — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded;
    /// see [`Client::reboot_eero`].
    ///
    /// Divergence from eero-api: Python invalidates nothing here, even though its sibling
    /// [`Client::set_led`] invalidates `eeros[{nid}_eeros]` for the exact same underlying node
    /// object — the behaviour brief calls this out as a likely oversight (gotcha G3), not a
    /// deliberate design choice. This port does not reproduce the gap: on success, this method
    /// also invalidates `eeros[{nid}_eeros]`, exactly like [`Client::set_led`] and
    /// [`Client::set_nightlight`] do. `rust-port-plan.md` §3.8 and `cache.rs`'s own module docs
    /// ("what's new" (b)) name this exact method as one of the two intentional improvements over
    /// Python this crate makes — do not remove this call later thinking it restores parity; it
    /// would reintroduce a bug, not fix one.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_led_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_led_brightness(eero_id, brightness)
            .await?;
        // Divergence from eero-api (rust-port-plan.md §3.8, improvement (b)): Python never
        // invalidates `eeros` after a brightness-only write; this port does, since the field it
        // just wrote lives in the same cached node object `set_led` already invalidates for.
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `set_nightlight` (`eero-api src/eero/client.py:1066-1094`). Every setting is
    /// optional and independent, exactly like
    /// [`crate::endpoints::EerosApi::set_nightlight`], which this delegates to unchanged — see
    /// that method's own docs for the brightness clamp and the request body shape. `auto_discover
    /// = false` — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded;
    /// see [`Client::reboot_eero`]. On success, invalidates `eeros[{nid}_eeros]`
    /// (`client.py:1090-1092`).
    ///
    /// Takes eight parameters (including the receiver), one more than
    /// [`crate::endpoints::EerosApi::set_nightlight`]'s seven, to additionally mirror
    /// `client.py:1066-1075`'s own `network_id` parameter — the same reasoning that method's own
    /// docs give for not splitting its six independent, self-describing scalar settings into a
    /// params struct: doing so would break the direct correspondence with the Python source
    /// without making any call site clearer.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "nightlight", .. }` if every one of `enabled`,
    /// `brightness`, `schedule_enabled`, `schedule_on`, `schedule_off` and
    /// `ambient_light_enabled` is `None` — see
    /// [`crate::endpoints::EerosApi::set_nightlight`]'s own docs for why this port refuses rather
    /// than fabricating Python's local `400` envelope. Otherwise see [`Client::get_diagnostics`].
    /// The cache is left untouched on any `Err`.
    #[allow(clippy::too_many_arguments)] // mirrors client.py:1066-1075's own 8-parameter signature, like EerosApi::set_nightlight's identical allow
    pub async fn set_nightlight(
        &self,
        eero_id: &str,
        enabled: Option<bool>,
        brightness: Option<i32>,
        schedule_enabled: Option<bool>,
        schedule_on: Option<&str>,
        schedule_off: Option<&str>,
        ambient_light_enabled: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight(
                eero_id,
                enabled,
                brightness,
                schedule_enabled,
                schedule_on,
                schedule_off,
                ambient_light_enabled,
            )
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets only the nightlight brightness for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `EerosAPI.set_nightlight_brightness`
    /// (`api/eeros.py:282`) on `EeroClient`, even though it wraps its sibling
    /// [`Client::set_nightlight`]. [`crate::endpoints::EerosApi::set_nightlight_brightness`]
    /// itself is a pure delegator to `EerosApi::set_nightlight` — the exact same wire call — so
    /// this method's cache behaviour simply follows suit: `auto_discover = false`, and on success
    /// invalidates `eeros[{nid}_eeros]`, identically to [`Client::set_nightlight`]. This is not a
    /// third invented improvement over Python; it is the same wire call `set_nightlight` already
    /// makes, which already invalidates that bucket.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. Never returns `Error::Validation`: a `brightness` value
    /// is always supplied here. The cache is left untouched on any `Err`.
    pub async fn set_nightlight_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight_brightness(eero_id, brightness)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets only the nightlight schedule for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent — see [`Client::set_nightlight_brightness`]'s docs, which apply
    /// identically here: [`crate::endpoints::EerosApi::set_nightlight_schedule`] delegates to the
    /// same `EerosApi::set_nightlight` call [`Client::set_nightlight`] itself uses, so this method
    /// follows the same cache behaviour: `auto_discover = false`, invalidates `eeros[{nid}_eeros]`
    /// on success.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. Never returns `Error::Validation`: `enabled` is always
    /// supplied here. The cache is left untouched on any `Err`.
    pub async fn set_nightlight_schedule(
        &self,
        eero_id: &str,
        enabled: bool,
        on_time: Option<&str>,
        off_time: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight_schedule(eero_id, enabled, on_time, off_time)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }
}
