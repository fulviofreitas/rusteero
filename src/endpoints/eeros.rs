//! `EerosApi` — Eero (Amazon mesh node) endpoints, ported from `eero-api`'s `EerosAPI`
//! (`src/eero/api/eeros.py:18-331`).
//!
//! Implements all ten `EerosAPI` methods: the four read (`GET`) methods —
//! [`EerosApi::get_eeros`], [`EerosApi::get_eero`], [`EerosApi::get_led_status`] and
//! [`EerosApi::get_nightlight`] — plus the six mutations — [`EerosApi::reboot_eero`],
//! [`EerosApi::set_led`], [`EerosApi::set_led_brightness`], [`EerosApi::set_nightlight`],
//! [`EerosApi::set_nightlight_brightness`] and [`EerosApi::set_nightlight_schedule`].
//!
//! ## The `network_id` parameter Python ignores
//!
//! Every Python method ported here except [`EerosApi::get_eeros`] declares a `network_id: str`
//! parameter that is never read; each method's own docstring says so explicitly (`"... (unused,
//! kept for API compatibility)"`, `eeros.py:57,78,101,127,159,192,225`), or — for the two
//! delegators, `set_nightlight_brightness` and `set_nightlight_schedule` — simply forwards a
//! `network_id` into `set_nightlight`, which itself never reads it. Every one of those methods
//! makes the exact same wire call, `eeros/{eero_id}` (not nested under `networks/`), regardless
//! of what `network_id` holds or whether it names a network the Eero even belongs to.
//!
//! `rusteero` drops the parameter from all of those signatures here rather than keeping a dead
//! one:
//!
//! - It carries no information any of these requests ever uses, on either side of the wire — the
//!   wire path is [`crate::routes::GET_EERO`] (or one of its PUT/POST siblings on the same
//!   `eeros/{eero_id}` resource) in every case, none of which has a `{network_id}` placeholder to
//!   fill.
//! - Keeping it would force every caller to supply a value that is silently discarded, with
//!   nothing at the type level warning them that it does nothing.
//! - [`EerosApi::get_eeros`] is the one method that *does* nest under
//!   `networks/{network_id}/eeros`, and keeps its `network_id` — the parameter is dropped only
//!   where Python itself never uses it, not across the whole module.
//!
//! This is a deliberate signature divergence from the Python source; see `PARITY.md` for where
//! it is recorded.
//!
//! ## Brightness is clamped, not validated
//!
//! [`EerosApi::set_led_brightness`] and the `brightness` field of [`EerosApi::set_nightlight`]
//! (and its [`EerosApi::set_nightlight_brightness`] delegator) reproduce Python's
//! `max(0, min(100, brightness))` clamp (`eeros.py:175,252`) verbatim: an out-of-range value is
//! silently pulled into `0..=100` before it is sent, never rejected with an error. See
//! [`EerosApi::set_led_brightness`]'s own docs for why its `brightness` parameter is a signed
//! `i32` rather than an unsigned or narrower type.
//!
//! ## The empty `set_nightlight` call diverges from Python
//!
//! Called with every optional field `None`, Python never sends a request at all: it logs a
//! warning and fabricates a *local* `{"meta": {"code": 400}, "data": {}}` envelope
//! (`eeros.py:269-272`) that never actually came from the server. That would violate this
//! crate's "the raw envelope is the contract, never transform or invent it" rule, so this port
//! diverges: an empty call returns `Error::Validation { field: "nightlight", .. }` before any
//! request is built — a `Result` already expresses "you passed nothing" honestly, without lying
//! about the wire. [`EerosApi::set_nightlight`]'s own docs cover this in detail.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// Eero (mesh node) endpoints. Ported from `EerosAPI` (`eero-api src/eero/api/eeros.py:18-31`).
///
/// Build one with [`EerosApi::new`], wrapping an already-configured [`Transport`] — typically
/// the same `Transport` shared with every other endpoint module behind the `EeroApi` aggregator.
#[derive(Debug)]
pub struct EerosApi {
    transport: Arc<Transport>,
}

impl EerosApi {
    /// Wraps `transport` as an `EerosApi`.
    ///
    /// Ported from `EerosAPI.__init__` (`eero-api src/eero/api/eeros.py:25-31`), which stores an
    /// `AuthAPI` handle rather than a `Transport` — `rusteero` centralises the authenticated
    /// request path in [`Transport`] itself (see `transport.rs`'s module docs), so every
    /// endpoint module holds one of those instead of a Python-shaped auth handle.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.get_eeros` (`eero-api src/eero/api/eeros.py:33-51`). Sends
    /// `GET` [`crate::routes::GET_EEROS`] with `network_id` filled into the
    /// `networks/{network_id}/eeros` path template, through [`Transport::send`] — which already
    /// implements the "not authenticated" precondition Python re-checks at the top of this
    /// method (`eeros.py:46-48`), so no separate guard is needed here.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_eeros(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_EEROS, &[("network_id", network_id)], None)
            .await
    }

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_eero` (`eero-api src/eero/api/eeros.py:53-72`); see the module
    /// docs for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_EERO`] with `eero_id` filled into the `eeros/{eero_id}` path
    /// template — **not** nested under `networks/` — through [`Transport::send`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_eero(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_EERO, &[("eero_id", eero_id)], None)
            .await
    }

    /// Gets LED status (`led_on`, `led_brightness`) for an Eero device — returns the raw Eero
    /// API response.
    ///
    /// Ported from `EerosAPI.get_led_status` (`eero-api src/eero/api/eeros.py:95-116`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_LED_STATUS`], an alias of [`crate::routes::GET_EERO`] — the exact
    /// same wire call as [`EerosApi::get_eero`]; the LED fields simply live in the same node
    /// object Python reads them out of client-side.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_led_status(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_LED_STATUS, &[("eero_id", eero_id)], None)
            .await
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Nightlight is only available on Eero Beacon devices; the raw response includes a
    /// `nightlight` object in `data` only when the device supports it. Ported from
    /// `EerosAPI.get_nightlight` (`eero-api src/eero/api/eeros.py:185-207`); see the module docs
    /// for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_NIGHTLIGHT`], an alias of [`crate::routes::GET_EERO`] — the exact
    /// same wire call as [`EerosApi::get_eero`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_nightlight(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_NIGHTLIGHT, &[("eero_id", eero_id)], None)
            .await
    }

    /// Reboots a single Eero device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.reboot_eero` (`eero-api src/eero/api/eeros.py:74-93`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter. Sends `POST`
    /// [`crate::routes::REBOOT_EERO`] with an empty JSON object body (`json={}`, matching
    /// `eeros.py:93`), through [`Transport::send`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn reboot_eero(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::REBOOT_EERO,
                &[("eero_id", eero_id)],
                Some(json!({})),
            )
            .await
    }

    /// Turns an Eero device's status LED on or off — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.set_led` (`eero-api src/eero/api/eeros.py:118-148`); see the module
    /// docs for why this port drops Python's unused `network_id` parameter. Sends `PUT`
    /// [`crate::routes::SET_LED`] with body `{"led_on": enabled}` (`eeros.py:144-148`), through
    /// [`Transport::send`]. This stays on API version 2.2 — unlike device nickname/pause writes,
    /// `EerosAPI` never switches to the `/2.3` base.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn set_led(&self, eero_id: &str, enabled: bool) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_LED,
                &[("eero_id", eero_id)],
                Some(json!({ "led_on": enabled })),
            )
            .await
    }

    /// Sets an Eero device's status LED brightness — returns the raw Eero API response.
    ///
    /// `brightness` is clamped to `0..=100` before it is sent, exactly like Python's
    /// `max(0, min(100, brightness))` (`eeros.py:175`) — an out-of-range value is silently
    /// clamped, never rejected with an error. `brightness` is a signed `i32` rather than an
    /// unsigned or narrower type deliberately: Python's `int` accepts negative values and relies
    /// on the clamp, not a type constraint, to correct them, so a caller must be able to *pass*
    /// e.g. `-5` for the clamp to have anything to do. `u8` would make that a compile-time
    /// impossibility instead of a runtime clamp, silently changing this method's behaviour
    /// relative to Python's; `i32` is wide enough to express any realistic out-of-range input
    /// (positive or negative) while staying a cheap `Copy` type.
    ///
    /// Ported from `EerosAPI.set_led_brightness` (`eero-api src/eero/api/eeros.py:150-183`); see
    /// the module docs for why this port drops Python's unused `network_id` parameter. Sends
    /// `PUT` [`crate::routes::SET_LED_BRIGHTNESS`] (an alias of [`crate::routes::SET_LED`]) with
    /// body `{"led_brightness": <0..=100>}` (`eeros.py:179-183`), through [`Transport::send`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn set_led_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_LED_BRIGHTNESS,
                &[("eero_id", eero_id)],
                Some(json!({ "led_brightness": brightness.clamp(0, 100) })),
            )
            .await
    }

    /// Sets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Every setting is optional and independent, exactly like Python's keyword-only arguments
    /// (`eeros.py:213-218`): only the fields actually supplied as `Some` are written into the
    /// request body, so a caller who only wants to flip `enabled` never has to know or guess the
    /// device's current brightness or schedule. `brightness` is clamped to `0..=100` the same
    /// way as [`EerosApi::set_led_brightness`] (`eeros.py:251-252`) — see that method's docs for
    /// why the parameter is a signed `i32`. The nested `schedule` object is built only when at
    /// least one of `schedule_enabled`, `schedule_on` or `schedule_off` is supplied
    /// (`eeros.py:258-267`), and, like the top-level object, only ever contains the keys that
    /// were actually supplied — never a `null` for an omitted one.
    ///
    /// The resulting body shape is:
    ///
    /// ```json
    /// {
    ///   "nightlight": {
    ///     "enabled": bool,               // present only if `enabled` was `Some`
    ///     "brightness": 0..=100,         // present only if `brightness` was `Some`
    ///     "ambient_light_enabled": bool, // present only if `ambient_light_enabled` was `Some`
    ///     "schedule": {                  // present only if any schedule_* field was `Some`
    ///       "enabled": bool,             // present only if `schedule_enabled` was `Some`
    ///       "on": "HH:MM",               // present only if `schedule_on` was `Some`
    ///       "off": "HH:MM"               // present only if `schedule_off` was `Some`
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Ported from `EerosAPI.set_nightlight` (`eero-api src/eero/api/eeros.py:209-280`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter. Sends `PUT`
    /// [`crate::routes::SET_NIGHTLIGHT`] (an alias of [`crate::routes::SET_LED`]) with the body
    /// shown above, through [`Transport::send`].
    ///
    /// Takes seven parameters (including the receiver) to mirror `eeros.py:209-219`'s six
    /// independent, optional keyword arguments one-to-one; splitting them into a params struct
    /// would break that direct call-signature correspondence with the Python source without
    /// making any call site clearer, since every field here is a plain, self-describing scalar.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_nightlight(
        &self,
        eero_id: &str,
        enabled: Option<bool>,
        brightness: Option<i32>,
        schedule_enabled: Option<bool>,
        schedule_on: Option<&str>,
        schedule_off: Option<&str>,
        ambient_light_enabled: Option<bool>,
    ) -> Result<Envelope, Error> {
        let mut nightlight = Map::new();

        if let Some(enabled) = enabled {
            nightlight.insert("enabled".to_string(), Value::Bool(enabled));
        }
        if let Some(brightness) = brightness {
            nightlight.insert(
                "brightness".to_string(),
                Value::from(brightness.clamp(0, 100)),
            );
        }
        if let Some(ambient_light_enabled) = ambient_light_enabled {
            nightlight.insert(
                "ambient_light_enabled".to_string(),
                Value::Bool(ambient_light_enabled),
            );
        }

        if schedule_enabled.is_some() || schedule_on.is_some() || schedule_off.is_some() {
            let mut schedule = Map::new();
            if let Some(schedule_enabled) = schedule_enabled {
                schedule.insert("enabled".to_string(), Value::Bool(schedule_enabled));
            }
            if let Some(schedule_on) = schedule_on {
                schedule.insert("on".to_string(), Value::String(schedule_on.to_string()));
            }
            if let Some(schedule_off) = schedule_off {
                schedule.insert("off".to_string(), Value::String(schedule_off.to_string()));
            }
            if !schedule.is_empty() {
                nightlight.insert("schedule".to_string(), Value::Object(schedule));
            }
        }

        // Divergence from `eeros.py:269-272` (see the module docs' "empty `set_nightlight`"
        // section): Python fabricates a local `{"meta": {"code": 400}, "data": {}}` envelope and
        // never touches the network; this port refuses before any request is built instead,
        // since a `Result` already says "you passed nothing" without inventing a fake response.
        if nightlight.is_empty() {
            return Err(Error::Validation {
                field: "nightlight".to_string(),
                message: "at least one nightlight setting must be provided".to_string(),
            });
        }

        let mut body = Map::new();
        body.insert("nightlight".to_string(), Value::Object(nightlight));

        self.transport
            .send(
                &routes::SET_NIGHTLIGHT,
                &[("eero_id", eero_id)],
                Some(Value::Object(body)),
            )
            .await
    }

    /// Sets only the nightlight brightness for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// A convenience delegator: sends the exact same request as
    /// `set_nightlight(eero_id, None, Some(brightness), None, None, None, None)`, so it shares
    /// that method's clamp and body shape (see [`EerosApi::set_nightlight`]'s docs) rather than
    /// building its own.
    ///
    /// Ported from `EerosAPI.set_nightlight_brightness` (`eero-api src/eero/api/eeros.py:282-300`);
    /// see the module docs for why this port drops Python's unused `network_id` parameter.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs). Never returns `Error::Validation`: a
    /// `brightness` value is always supplied here, so [`EerosApi::set_nightlight`]'s empty-body
    /// guard can never trigger.
    pub async fn set_nightlight_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, Some(brightness), None, None, None, None)
            .await
    }

    /// Sets only the nightlight schedule for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// A convenience delegator: sends the exact same request as
    /// `set_nightlight(eero_id, None, None, Some(enabled), on_time, off_time, None)`, so it
    /// shares that method's body shape (see [`EerosApi::set_nightlight`]'s docs) rather than
    /// building its own.
    ///
    /// Ported from `EerosAPI.set_nightlight_schedule` (`eero-api src/eero/api/eeros.py:302-330`);
    /// see the module docs for why this port drops Python's unused `network_id` parameter.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs). Never returns `Error::Validation`: `enabled`
    /// is always supplied here, so [`EerosApi::set_nightlight`]'s empty-body guard can never
    /// trigger.
    pub async fn set_nightlight_schedule(
        &self,
        eero_id: &str,
        enabled: bool,
        on_time: Option<&str>,
        off_time: Option<&str>,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, None, Some(enabled), on_time, off_time, None)
            .await
    }
}
