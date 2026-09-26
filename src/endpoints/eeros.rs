//! `EerosApi` — Eero (Amazon mesh node) endpoints, ported from `eero-api`'s `EerosAPI`
//! (`src/eero/api/eeros.py:18-863` at v8.0.4).
//!
//! Implements all seventeen `EerosAPI` methods: the seven reads —
//! [`EerosApi::get_eeros`], [`EerosApi::get_eero`], [`EerosApi::get_led_status`],
//! [`EerosApi::get_connections`], [`EerosApi::get_nightlight`], [`EerosApi::get_eero_support`] —
//! plus the ten mutations — [`EerosApi::reboot_eero`], [`EerosApi::set_location`],
//! [`EerosApi::set_led`], [`EerosApi::set_led_brightness`], [`EerosApi::set_nightlight`],
//! [`EerosApi::set_nightlight_brightness`], [`EerosApi::set_nightlight_schedule`],
//! [`EerosApi::node_action`], [`EerosApi::port_action`], [`EerosApi::led_cycle`],
//! [`EerosApi::nightlight_override`].
//!
//! ## `network_id` is dropped from every method except `get_eeros`
//!
//! Every Python method here except `get_eeros` either declares a `network_id: str` parameter
//! that is never read for URL resolution (its own docstring says so explicitly — `"... (unused,
//! kept for API compatibility)"`), or — `node_action`, `port_action`, `led_cycle`,
//! `nightlight_override`, `get_eero_support` — never declares the parameter at all. This port
//! drops the parameter wherever Python itself never uses it, matching the crate-wide convention
//! recorded in `PARITY.md`; only [`EerosApi::get_eeros`] keeps it, since it is the one method
//! that nests its wire path under `networks/{network_id}/eeros`.
//!
//! ## Brightness is validated, not clamped (v8.0.4 change from v6.2.0)
//!
//! [`EerosApi::set_led_brightness`], the `brightness_percentage` field of
//! [`EerosApi::set_nightlight`] (and its [`EerosApi::set_nightlight_brightness`] delegator), and
//! [`EerosApi::nightlight_override`] all reject an out-of-range value with
//! `Error::Validation { field, .. }` — unlike the pre-v8.0.0 shape this crate used to reproduce,
//! `_validate_brightness` (`eeros.py:105-122`) raises rather than clamping.
//!
//! ## `set_led`/`set_led_brightness`: new URL, form encoding, string literals (v8.0.4 change)
//!
//! The pre-v8.0.0 shape — a JSON `PUT` to the eero's own URL — was verified to change nothing
//! server-side (`wiki/Migration.md:451`). At v8.0.4 both methods `PUT` a dedicated `eeros/{id}/led`
//! sub-resource, form-encoded, with `led_on`/`led_brightness` sent as string literals
//! (`"true"`/`"false"`, `"42"`), not JSON `true`/`false`/`42` — see [`crate::routes::eeros::SET_LED`].
//!
//! ## Nightlight discovery, not a template (`get_nightlight`/`set_nightlight`)
//!
//! Neither method builds `eeros/{id}/nightlight` from a fixed template. [`EerosApi::get_nightlight`]
//! and [`EerosApi::set_nightlight`] both call `resolve_nightlight_url`: prefer a
//! parent envelope's own `data.nightlight.url` field; otherwise issue one live `GET` of the
//! eero's own URL (ignoring `parent` for that fetch, exactly like Python — if `parent` had a
//! usable nightlight URL it would already have been found) and read `nightlight.url` off the
//! fresh response. If that fresh response also carries no `nightlight.url`, this returns
//! `Error::FeatureUnavailable` — the request never even reaches an "eero doesn't support
//! nightlight" server response, because there may be none. Ported from `eeros.py:443-478`
//! (`_resolve_nightlight_url`, inlined here as a private async helper since Rust has no
//! equivalent of a bare Python module-level `async def`).
//!
//! [`EerosApi::set_nightlight`] validates its own fields (brightness range, "at least one field
//! supplied") **before** calling `resolve_nightlight_url` — an out-of-range
//! `brightness_percentage` on a non-Beacon eero therefore surfaces as `Error::Validation`, not
//! `Error::FeatureUnavailable`, exactly matching `eeros.py:531-547`'s ordering.

use std::sync::Arc;

use reqwest::Method;
use serde_json::{Map, Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes::{self, ApiVersion, Resource};
use crate::transport::{RequestBody, Transport};

/// Valid values for [`EerosApi::node_action`]'s `action` parameter.
///
/// Ported from `_NODE_ACTIONS` (`eero-api src/eero/api/eeros.py:34`).
const NODE_ACTIONS: &[&str] = &["POWER_CYCLE_ALL_PORTS", "POWER_CYCLE_ALL_PORTS_AND_REBOOT"];

/// Valid values for [`EerosApi::port_action`]'s `action` parameter.
///
/// Ported from `_PORT_ACTIONS` (`eero-api src/eero/api/eeros.py:37-49`).
const PORT_ACTIONS: &[&str] = &[
    "ENABLE_DATA",
    "DISABLE_DATA",
    "ENABLE_POE",
    "DISABLE_POE",
    "ENABLE_PORT",
    "DISABLE_PORT",
    "RESTART_POWER",
    "ENABLE_PORT_SECURITY",
    "DISABLE_PORT_SECURITY",
];

/// Resolves `route`'s URL the self-url-preferred way: `parent`'s own `url` field
/// ([`crate::links::self_url`]) when present, else `route.template` filled in from `id_or_url`
/// ([`crate::links::resource_url`]).
///
/// This is a different preference order than [`Resource::resolve`] (which only ever prefers a
/// named `resources` link, never a bare `url` field), so `get_eero`/`get_led_status`/
/// `set_location` call this directly instead of `route.resolve(..)`. Ported from
/// `_eero_own_url` (`eero-api src/eero/api/eeros.py:52-67`).
fn resolve_self_preferred(
    host: &Url,
    id_or_url: &str,
    route: &Resource,
    parent: Option<&Value>,
) -> Result<Url, Error> {
    if let Some(parent) = parent
        && let Some(url) = crate::links::self_url(host, parent)?
    {
        return Ok(url);
    }
    crate::links::resource_url(host, id_or_url, route.template, route.version)
}

/// Reads a `data.nightlight.url` field out of `value` (a full envelope or a bare `data` object)
/// and joins it onto `host`, if present and non-empty.
///
/// Ported from `_nightlight_url_from_envelope` (`eero-api src/eero/api/eeros.py:70-102`).
fn nightlight_url_from_value(host: &Url, value: &Value) -> Result<Option<Url>, Error> {
    let data = value
        .get("data")
        .filter(|data| data.is_object())
        .unwrap_or(value);
    let Some(url) = data
        .get("nightlight")
        .and_then(|nightlight| nightlight.get("url"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    if url.is_empty() {
        return Ok(None);
    }
    crate::links::join_api_path(host, url).map(Some)
}

/// Validates that `value` is an integer in `0..=100`.
///
/// Ported from `_validate_brightness` (`eero-api src/eero/api/eeros.py:105-122`) — the Python
/// `bool`-before-`int` check has no Rust equivalent, since `brightness: i32` is statically never
/// a `bool`.
///
/// # Errors
///
/// Returns [`Error::validation`] with the fixed message `"must be an integer between 0 and 100"`
/// if `value` is outside `0..=100`.
fn validate_brightness(value: i32, field: &str) -> Result<i32, Error> {
    if (0..=100).contains(&value) {
        Ok(value)
    } else {
        Err(Error::validation(
            field,
            "must be an integer between 0 and 100",
        ))
    }
}

/// Eero (mesh node) endpoints. Ported from `EerosAPI` (`eero-api src/eero/api/eeros.py:18-863`).
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
    /// Ported from `EerosAPI.__init__` (`eero-api src/eero/api/eeros.py:157-165`), which stores
    /// an `AuthAPI` handle rather than a `Transport` — see [`Transport`]'s own module docs for
    /// why every endpoint module here holds one of those instead.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.get_eeros` (`eero-api src/eero/api/eeros.py:168-195`). Sends `GET`
    /// [`crate::routes::eeros::GET_EEROS`], preferring `parent`'s `resources.eeros` link over
    /// the `networks/{id}/eeros` template.
    pub async fn get_eeros(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_EEROS,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_eero` (`eero-api src/eero/api/eeros.py:197-225`); see the module
    /// docs for why this port drops Python's unused `network_id` parameter. Prefers `parent`'s
    /// own `url` field over the `eeros/{id}` template (`resolve_self_preferred`).
    pub async fn get_eero(&self, eero_id: &str, parent: Option<&Value>) -> Result<Envelope, Error> {
        let url = resolve_self_preferred(
            self.transport.api_host(),
            eero_id,
            &routes::GET_EERO,
            parent,
        )?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Gets LED status (`led_on`, `led_brightness`) for an Eero device — returns the raw Eero
    /// API response.
    ///
    /// Ported from `EerosAPI.get_led_status` (`eero-api src/eero/api/eeros.py:271-300`); resolves
    /// exactly like [`EerosApi::get_eero`] — the LED fields simply live in the same node object.
    pub async fn get_led_status(
        &self,
        eero_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = resolve_self_preferred(
            self.transport.api_host(),
            eero_id,
            &routes::GET_LED_STATUS,
            parent,
        )?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Sets an Eero device's Wi-Fi location label — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.set_location` (`eero-api src/eero/api/eeros.py:401-441`); resolves
    /// like [`EerosApi::get_eero`]. Sends a form-encoded `PUT` with `location=<location>`.
    /// **Unverified against a live network.**
    pub async fn set_location(
        &self,
        eero_id: &str,
        location: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = resolve_self_preferred(
            self.transport.api_host(),
            eero_id,
            &routes::SET_LOCATION,
            parent,
        )?;
        crate::links::warn_uncharacterised_write("set location for eero");
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Form(vec![("location".to_owned(), location.to_owned())]),
            )
            .await
    }

    /// Reboots a single Eero device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.reboot_eero` (`eero-api src/eero/api/eeros.py:227-269`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter. Sends `POST`
    /// [`crate::routes::eeros::REBOOT_EERO`] with the literal body `""`
    /// (`RequestBody::EmptyJsonString`), matching `eeros.py:266`. **Live-verified 2026-09-20**:
    /// HTTP 201, only the targeted eero rebooted.
    pub async fn reboot_eero(
        &self,
        eero_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::REBOOT_EERO,
                eero_id,
                parent,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// Turns an Eero device's status LED on or off — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.set_led` (`eero-api src/eero/api/eeros.py:302-350`); see the module
    /// docs for why this port drops Python's unused `network_id` parameter, and for the v8.0.4
    /// wire-format change (new URL, form encoding, string literals). **Live-verified
    /// 2026-09-20.**
    pub async fn set_led(
        &self,
        eero_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let value = if enabled { "true" } else { "false" }.to_owned();
        self.transport
            .resource(
                &routes::SET_LED,
                eero_id,
                parent,
                &[],
                RequestBody::Form(vec![("led_on".to_owned(), value)]),
            )
            .await
    }

    /// Sets an Eero device's status LED brightness — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.set_led_brightness` (`eero-api src/eero/api/eeros.py:352-399`); see
    /// the module docs for the v8.0.4 changes (validated not clamped, new URL, form encoding).
    /// **Live-verified 2026-09-20.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "brightness", .. }` if `brightness` is outside
    /// `0..=100`. Otherwise as every other method here.
    pub async fn set_led_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let brightness = validate_brightness(brightness, "brightness")?;
        self.transport
            .resource(
                &routes::SET_LED_BRIGHTNESS,
                eero_id,
                parent,
                &[],
                RequestBody::Form(vec![("led_brightness".to_owned(), brightness.to_string())]),
            )
            .await
    }

    /// Gets a single Eero device's client connections — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_connections` (`eero-api src/eero/api/eeros.py:606-639`).
    pub async fn get_connections(
        &self,
        eero_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_CONNECTIONS,
                eero_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Resolves the nightlight sub-resource URL for `eero_id`, per this module's own "Nightlight
    /// discovery" docs.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FeatureUnavailable`] with `feature`-equivalent text `"not available on
    /// this eero"` if neither `parent` nor a freshly-fetched eero carries a `nightlight.url`
    /// field. Otherwise propagates whatever the discovery `GET` itself returns.
    async fn resolve_nightlight_url(
        &self,
        eero_id: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent
            && let Some(url) = nightlight_url_from_value(host, parent)?
        {
            return Ok(url);
        }
        let own_url = resolve_self_preferred(host, eero_id, &routes::GET_EERO, None)?;
        let envelope = self
            .transport
            .request(Method::GET, own_url, &[], RequestBody::None)
            .await?;
        if let Some(url) = nightlight_url_from_value(host, envelope.as_value())? {
            return Ok(url);
        }
        Err(Error::FeatureUnavailable {
            status: None,
            message: "not available on this eero".to_owned(),
            envelope: None,
            error_code: None,
        })
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_nightlight` (`eero-api src/eero/api/eeros.py:443-478`); see the
    /// module docs' "Nightlight discovery" section.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FeatureUnavailable`] if the eero has no nightlight — see
    /// `resolve_nightlight_url`. Otherwise as every other method here.
    pub async fn get_nightlight(
        &self,
        eero_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_nightlight_url(eero_id, parent).await?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Sets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Every setting is optional and independent, exactly like Python's keyword-only arguments
    /// (`eeros.py:480-489`): only the fields actually supplied as `Some` are written into the
    /// request body. `schedule` is forwarded completely unchanged — this port applies no shape
    /// interpretation to it, matching Python's own `Any` typing (`eeros.py:483`).
    ///
    /// Ported from `EerosAPI.set_nightlight` (`eero-api src/eero/api/eeros.py:480-549`); see the
    /// module docs for the validate-before-discover ordering this reproduces exactly, and for the
    /// v8.0.4 field-set replacement (`enabled`/`brightness_percentage`/`schedule`, not the old
    /// `brightness`/`schedule_enabled`/`schedule_on`/`schedule_off`/`ambient_light_enabled`
    /// shape). Sends `PUT` to the resource `resolve_nightlight_url` discovers, with a
    /// top-level (not nested) JSON body. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "brightness_percentage", .. }` if that value is
    /// outside `0..=100`. Returns `Error::Validation { field: "nightlight", .. }` if `enabled`,
    /// `brightness_percentage` and `schedule` are all `None`. Returns
    /// [`Error::FeatureUnavailable`] if the eero has no nightlight. Otherwise as every other
    /// method here.
    pub async fn set_nightlight(
        &self,
        eero_id: &str,
        enabled: Option<bool>,
        brightness_percentage: Option<i32>,
        schedule: Option<Value>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let brightness_percentage = brightness_percentage
            .map(|value| validate_brightness(value, "brightness_percentage"))
            .transpose()?;

        let mut body = Map::new();
        if let Some(enabled) = enabled {
            body.insert("enabled".to_owned(), Value::Bool(enabled));
        }
        if let Some(brightness_percentage) = brightness_percentage {
            body.insert(
                "brightness_percentage".to_owned(),
                Value::from(brightness_percentage),
            );
        }
        if let Some(schedule) = schedule {
            body.insert("schedule".to_owned(), schedule);
        }
        if body.is_empty() {
            return Err(Error::validation(
                "nightlight",
                "at least one of enabled, brightness_percentage, schedule must be supplied",
            ));
        }

        let url = self.resolve_nightlight_url(eero_id, parent).await?;
        crate::links::warn_uncharacterised_write("set nightlight for eero");
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// Sets only the nightlight brightness for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// A convenience delegator: sends the exact same request as
    /// `set_nightlight(eero_id, None, Some(brightness_percentage), None, parent)`.
    ///
    /// Ported from `EerosAPI.set_nightlight_brightness` (`eero-api src/eero/api/eeros.py:551-579`).
    ///
    /// # Errors
    ///
    /// See [`EerosApi::set_nightlight`]. Never returns the "at least one field" validation error:
    /// a `brightness_percentage` value is always supplied here.
    pub async fn set_nightlight_brightness(
        &self,
        eero_id: &str,
        brightness_percentage: i32,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, Some(brightness_percentage), None, parent)
            .await
    }

    /// Sets only the nightlight schedule for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// A convenience delegator: sends the exact same request as
    /// `set_nightlight(eero_id, None, None, Some(schedule), parent)`. `schedule` is forwarded
    /// completely unchanged.
    ///
    /// Ported from `EerosAPI.set_nightlight_schedule` (`eero-api src/eero/api/eeros.py:581-604`).
    ///
    /// # Errors
    ///
    /// See [`EerosApi::set_nightlight`]. Never returns the "at least one field" validation error:
    /// `schedule` is always supplied here.
    pub async fn set_nightlight_schedule(
        &self,
        eero_id: &str,
        schedule: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.set_nightlight(eero_id, None, None, Some(schedule), parent)
            .await
    }

    /// Performs a node-level action on an Eero device (power-cycling its ports, optionally
    /// rebooting the node too) — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.node_action` (`eero-api src/eero/api/eeros.py:641-693`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter (it has none
    /// here at all). **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "action", .. }` if `action` is not one of
    /// `"POWER_CYCLE_ALL_PORTS"`/`"POWER_CYCLE_ALL_PORTS_AND_REBOOT"`. Otherwise as every other
    /// method here.
    pub async fn node_action(
        &self,
        eero_id: &str,
        action: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if !NODE_ACTIONS.contains(&action) {
            return Err(Error::validation(
                "action",
                format!("must be one of {NODE_ACTIONS:?}, got {action:?}"),
            ));
        }
        crate::links::warn_uncharacterised_write(
            "perform node action on eero -- power-cycles ports and, for \
             POWER_CYCLE_ALL_PORTS_AND_REBOOT, reboots the eero",
        );
        self.transport
            .resource(
                &routes::NODE_ACTION,
                eero_id,
                parent,
                &[],
                RequestBody::Json(json!({ "action": action })),
            )
            .await
    }

    /// Performs a port-level action on an Eero device's interface — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.port_action` (`eero-api src/eero/api/eeros.py:695-742`); no
    /// `network_id`, no `parent=` in Python — the URL is built by hand from
    /// [`crate::links::resource_url`] + [`crate::links::child_url`] plus a literal `"/action"`
    /// suffix, since two distinct ids (`eero_id`, `interface_number`) cannot be expressed by a
    /// single [`Resource`]. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "action", .. }` if `action` is not one of the nine
    /// recognised port actions. Otherwise as every other method here.
    pub async fn port_action(
        &self,
        eero_id: &str,
        interface_number: u32,
        action: &str,
    ) -> Result<Envelope, Error> {
        if !PORT_ACTIONS.contains(&action) {
            return Err(Error::validation(
                "action",
                format!("must be one of {PORT_ACTIONS:?}, got {action:?}"),
            ));
        }
        crate::links::warn_uncharacterised_write("perform port action on eero port");

        let host = self.transport.api_host();
        let ports_base =
            crate::links::resource_url(host, eero_id, "eeros/{id}/ports", ApiVersion::V2_2)?;
        let member = crate::links::child_url(&ports_base, &interface_number.to_string())?;
        let url = Url::parse(&format!("{}/action", member.as_str().trim_end_matches('/')))
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))?;

        self.transport
            .request(
                Method::POST,
                url,
                &[],
                RequestBody::Json(json!({ "action": action })),
            )
            .await
    }

    /// Cycles an Eero device's status LED through a colour sequence — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.led_cycle` (`eero-api src/eero/api/eeros.py:744-792`); no
    /// `network_id`, no `parent=` in Python — addressed by `eero_serial` alone. Sends a
    /// form-encoded body with one repeated `colors[]` field per entry in `colors`, matching
    /// aiohttp's own list-form encoding (`data={"colors[]": colors, ...}`,
    /// `eeros.py:787-791`) rather than a single JSON-array-shaped value. **Unverified against a
    /// live network.**
    pub async fn led_cycle(
        &self,
        eero_serial: &str,
        colors: &[String],
        duration: u32,
        time_per_color: u32,
    ) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("cycle LED for eero");
        let mut pairs: Vec<(String, String)> = colors
            .iter()
            .map(|color| ("colors[]".to_owned(), color.clone()))
            .collect();
        pairs.push(("duration".to_owned(), duration.to_string()));
        pairs.push(("time_per_color".to_owned(), time_per_color.to_string()));

        self.transport
            .resource(
                &routes::LED_CYCLE,
                eero_serial,
                None,
                &[],
                RequestBody::Form(pairs),
            )
            .await
    }

    /// Previews a nightlight brightness value without persisting it — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.nightlight_override` (`eero-api src/eero/api/eeros.py:794-834`); no
    /// `network_id`, no `parent=` in Python. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "brightness_percentage", .. }` if `brightness_percentage`
    /// is outside `0..=100`. Otherwise as every other method here.
    pub async fn nightlight_override(
        &self,
        eero_id: &str,
        brightness_percentage: i32,
    ) -> Result<Envelope, Error> {
        let brightness_percentage =
            validate_brightness(brightness_percentage, "brightness_percentage")?;
        crate::links::warn_uncharacterised_write("override nightlight preview for eero");
        self.transport
            .resource(
                &routes::NIGHTLIGHT_OVERRIDE,
                eero_id,
                None,
                &[],
                RequestBody::Form(vec![(
                    "brightness_percentage".to_owned(),
                    brightness_percentage.to_string(),
                )]),
            )
            .await
    }

    /// Gets an Eero device's support/diagnostics summary — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_eero_support` (`eero-api src/eero/api/eeros.py:836-863`); no
    /// `network_id`, no `parent=` in Python — addressed by `eero_serial` alone. Observed to `404`
    /// on at least one live node.
    pub async fn get_eero_support(&self, eero_serial: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_EERO_SUPPORT,
                eero_serial,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }
}
