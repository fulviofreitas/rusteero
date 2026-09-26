//! `Wpa3Api`: per-band WPA3 settings API (`eero-api`'s `Wpa3API`), v8.0.4.
//!
//! Ported from `eero-api src/eero/api/wpa3.py` at v8.0.4 — a module with no `v6.2.0`
//! predecessor. Every method here funnels through [`crate::transport::Transport::resource`],
//! which already implements the "not authenticated" precondition and every status-to-error
//! mapping a response can produce.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::{py_list, py_quote};
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// Valid values for each band's WPA3 mode.
///
/// Ported from `WPA3_MODE_WPA2`/`WPA3_MODE_WPA2_WPA3`/`WPA3_MODE_WPA3`/`_WPA3_MODES`
/// (`wpa3.py:20-23`).
pub const WPA3_MODES: &[&str] = &["WPA2", "WPA2_WPA3", "WPA3"];

/// `eero-api`'s `Wpa3API` (`src/eero/api/wpa3.py`), new in v8.0.0.
///
/// Build one with [`Wpa3Api::new`], wrapping a [`Transport`] shared with the rest of the
/// [`crate::api::EeroApi`] aggregator — `Wpa3Api` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct Wpa3Api {
    transport: Arc<Transport>,
}

impl Wpa3Api {
    /// Wraps `transport` as a `Wpa3Api`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the per-band WPA3 mode — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wpa3.py:60-93` (`Wpa3API.get_wpa3_per_band`). GETs the
    /// network's `wpa3_per_band` sub-resource. A verified read. The response may carry a third
    /// key, `band_6_ghz`, that [`Wpa3Api::set_wpa3_per_band`] has no parameter for — the read and
    /// write key sets are not symmetric (g5 brief §4).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_wpa3_per_band(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::wpa3::GET_WPA3_PER_BAND,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets the per-band WPA3 mode — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wpa3.py:95-158` (`Wpa3API.set_wpa3_per_band`). Issues a
    /// JSON PUT to the network's `wpa3_per_band` sub-resource with exactly the bands supplied
    /// (`wpa3.py:141-144`) — a band omitted (`None`) is left out of the request body entirely,
    /// never sent as `null`. Unverified upstream: may require devices to reconnect if their
    /// negotiated security mode is no longer offered.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` with the offending field's name if a supplied value is not one
    /// of [`WPA3_MODES`] (`wpa3.py:29-42`), or with `field: "wpa3_per_band"` if both
    /// `band_2_4_ghz` and `band_5_ghz` are `None` (`wpa3.py:146-149`) — before any request is
    /// sent. Otherwise see [`Wpa3Api::get_wpa3_per_band`].
    pub async fn set_wpa3_per_band(
        &self,
        network_id: &str,
        band_2_4_ghz: Option<&str>,
        band_5_ghz: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        if let Some(mode) = band_2_4_ghz {
            payload.insert(
                "band_2_4_ghz".to_owned(),
                Value::String(validate_mode(mode, "band_2_4_ghz")?.to_owned()),
            );
        }
        if let Some(mode) = band_5_ghz {
            payload.insert(
                "band_5_ghz".to_owned(),
                Value::String(validate_mode(mode, "band_5_ghz")?.to_owned()),
            );
        }
        if payload.is_empty() {
            return Err(Error::validation(
                "wpa3_per_band",
                "at least one of band_2_4_ghz, band_5_ghz must be supplied",
            ));
        }

        let url = routes::wpa3::SET_WPA3_PER_BAND.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set per-band WPA3 mode for network");
        self.transport
            .request(
                routes::wpa3::SET_WPA3_PER_BAND.method.clone(),
                url,
                &[],
                RequestBody::Json(Value::Object(payload)),
            )
            .await
    }
}

/// Validates a WPA3-per-band mode value.
///
/// Ported from `_validate_mode` (`wpa3.py:29-42`).
///
/// # Errors
///
/// Returns `Error::Validation` with `field` as `field` if `value` is not one of [`WPA3_MODES`].
fn validate_mode<'a>(value: &'a str, field: &str) -> Result<&'a str, Error> {
    if !WPA3_MODES.contains(&value) {
        return Err(Error::validation(
            field,
            format!(
                "must be one of {}, got {}",
                py_list(WPA3_MODES),
                py_quote(value)
            ),
        ));
    }
    Ok(value)
}
