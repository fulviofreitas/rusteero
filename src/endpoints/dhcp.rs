//! `DhcpApi`: `dhcp` endpoints (`eero-api src/eero/api/dhcp.py`, new in v8.0.0).
//!
//! Every write in this module goes to the network's `settings` sub-resource (the same endpoint
//! used by `NetworksApi`/`DnsApi`/`SecurityApi`/`SqmApi`), with the `dhcp`, `connection`, and
//! `nat_port_randomization` fields declared for that endpoint. None of the connection-mode or NAT
//! writes have been confirmed against a live network, and DHCP/connection-mode changes are
//! settings-class writes: the DNS write path on this same endpoint is confirmed to reboot the
//! entire mesh (every eero restarts, all clients drop), and this SDK treats every other
//! settings-class write as capable of the same behaviour until proven otherwise. The DHCP write
//! is the one that has since been confirmed: see [`DhcpApi::set_dhcp`].

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::params::{py_list, py_quote};
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// The `dhcp.mode` that hands out leases automatically (`eero-api src/eero/api/dhcp.py:32`,
/// `DHCP_MODE_AUTOMATIC`).
pub const DHCP_MODE_AUTOMATIC: &str = "automatic";

/// The `dhcp.mode` for a manual lease range, kept for backward compatibility (`dhcp.py:33`,
/// `DHCP_MODE_MANUAL`). The API refuses a literal `"manual"`, so it goes out on the wire as
/// [`DHCP_MODE_CUSTOM`].
pub const DHCP_MODE_MANUAL: &str = "manual";

/// The wire value the API accepts for a manual lease range (`dhcp.py:38`, `DHCP_MODE_CUSTOM`),
/// also accepted from callers directly.
pub const DHCP_MODE_CUSTOM: &str = "custom";

/// Valid values for `dhcp.mode` (`dhcp.py:42`, `_DHCP_MODES`).
pub const DHCP_MODES: &[&str] = &[DHCP_MODE_AUTOMATIC, DHCP_MODE_MANUAL, DHCP_MODE_CUSTOM];

/// Maps every mode a caller may pass to the value sent on the wire (`dhcp.py:45-49`,
/// `_DHCP_MODE_WIRE_VALUES`): `"manual"` is sent as `"custom"`.
const DHCP_MODE_WIRE_VALUES: &[(&str, &str)] = &[
    (DHCP_MODE_AUTOMATIC, DHCP_MODE_AUTOMATIC),
    (DHCP_MODE_MANUAL, DHCP_MODE_CUSTOM),
    (DHCP_MODE_CUSTOM, DHCP_MODE_CUSTOM),
];

/// Valid values for `DhcpApi::set_connection_mode`'s `mode`
/// (`eero-api src/eero/api/dhcp.py:228`, `valid_modes`).
pub const CONNECTION_MODES: &[&str] = &["BRIDGE", "NAT"];

/// Fields the API accepts on a manual DHCP lease range (`dhcp.custom`).
///
/// Ported from `_CUSTOM_LEASE_FIELDS` (`eero-api src/eero/api/dhcp.py:52`).
const CUSTOM_LEASE_FIELDS: &[&str] = &["start_ip", "end_ip", "subnet_ip", "subnet_mask"];

/// Fields the API accepts on the per-subnet DHCP configuration (`dhcp.custom_v2`).
///
/// Ported from `_CUSTOM_V2_FIELDS` (`eero-api src/eero/api/dhcp.py:56`).
const CUSTOM_V2_FIELDS: &[&str] = &["main", "guest", "subnetA", "subnetB", "supernet"];

/// Returns the wire value for a caller-supplied `dhcp.mode`, or `None` if it is not one of
/// [`DHCP_MODES`] (the lookup is case-sensitive).
fn wire_mode(mode: &str) -> Option<&'static str> {
    DHCP_MODE_WIRE_VALUES
        .iter()
        .find(|(accepted, _)| *accepted == mode)
        .map(|&(_, wire)| wire)
}

/// `eero-api`'s `DhcpAPI` (`src/eero/api/dhcp.py`, new in v8.0.0).
///
/// Build one with [`DhcpApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `DhcpApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct DhcpApi {
    transport: Arc<Transport>,
}

impl DhcpApi {
    /// Wraps `transport` as a `DhcpApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Sets the network's DHCP configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dhcp.py:102-185` (`DhcpAPI.set_dhcp`). Issues a JSON
    /// `PUT` to [`crate::routes::dhcp::DHCP_SET_DHCP`] with exactly the `dhcp` sub-fields
    /// supplied among `mode`, `custom`, and `custom_v2`. Logs
    /// [`crate::links::warn_uncharacterised_write`] with `"set DHCP configuration for network --
    /// may reboot every eero, like the confirmed DNS write path"` immediately before issuing the
    /// request.
    ///
    /// `mode` is `"automatic"`, `"manual"` or `"custom"`. `"manual"` is accepted for backward
    /// compatibility and sent as `"custom"`, the wire value the API actually accepts: it refuses
    /// a literal `"manual"` with `400 dhcp.mode: error.form.enum.invalid` (`eero-api` issue
    /// #136).
    ///
    /// # Warning
    ///
    /// This is a settings-class write, and its cost is confirmed rather than suspected: live
    /// testing found that each DHCP write restarts every eero on the mesh, an outage of roughly
    /// eight minutes each time — the same as the already-confirmed DNS write on this endpoint.
    /// Read the current configuration first (`NetworksApi::get_network`), skip the write when it
    /// already matches, and never retry a failed write in a loop.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "mode"` if `mode` is supplied and is not
    /// `"automatic"`, `"manual"` or `"custom"` (the comparison is case-sensitive). Returns
    /// [`Error::Validation`] with `field: "field"` if `custom`/`custom_v2` carries a key outside
    /// the fields the API declares for it. Returns [`Error::Validation`] with `field: "dhcp"` if
    /// none of `mode`, `custom`, `custom_v2` is supplied. Returns [`Error::Authentication`] if no
    /// valid session is configured, or whatever status-mapped [`Error`] the request produces
    /// otherwise.
    pub async fn set_dhcp(
        &self,
        network_id: &str,
        mode: Option<&str>,
        custom: Option<&Map<String, Value>>,
        custom_v2: Option<&Map<String, Value>>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut dhcp = Map::new();
        if let Some(mode) = mode {
            let Some(wire) = wire_mode(mode) else {
                return Err(Error::validation(
                    "mode",
                    format!(
                        "must be one of {}, got {}",
                        py_list(DHCP_MODES),
                        py_quote(mode)
                    ),
                ));
            };
            dhcp.insert("mode".to_owned(), Value::String(wire.to_owned()));
        }
        if let Some(custom) = custom {
            dhcp.insert(
                "custom".to_owned(),
                Value::Object(filter_given_keys(custom, CUSTOM_LEASE_FIELDS)?),
            );
        }
        if let Some(custom_v2) = custom_v2 {
            dhcp.insert(
                "custom_v2".to_owned(),
                Value::Object(filter_given_keys(custom_v2, CUSTOM_V2_FIELDS)?),
            );
        }
        if dhcp.is_empty() {
            return Err(Error::validation(
                "dhcp",
                "at least one of mode, custom, custom_v2 must be supplied",
            ));
        }

        links::warn_uncharacterised_write(
            "set DHCP configuration for network -- may reboot every eero, like the confirmed \
             DNS write path",
        );
        self.transport
            .resource(
                &routes::dhcp::DHCP_SET_DHCP,
                network_id,
                parent,
                &[],
                RequestBody::Json(json!({ "dhcp": dhcp })),
            )
            .await
    }

    /// Sets the network's WAN connection mode — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dhcp.py:187-242` (`DhcpAPI.set_connection_mode`).
    /// Issues a JSON `PUT` to [`crate::routes::dhcp::DHCP_SET_CONNECTION_MODE`] with
    /// `{"connection": {"mode": mode}}`. Logs [`crate::links::warn_uncharacterised_write`] with
    /// `"set connection mode for network -- may reboot every eero, like the confirmed DNS write
    /// path"` immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "mode"` if `mode` is not `"BRIDGE"` or
    /// `"NAT"`. Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_connection_mode(
        &self,
        network_id: &str,
        mode: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if !CONNECTION_MODES.contains(&mode) {
            return Err(Error::validation(
                "mode",
                format!(
                    "must be one of {}, got {}",
                    py_list(CONNECTION_MODES),
                    py_quote(mode)
                ),
            ));
        }
        links::warn_uncharacterised_write(
            "set connection mode for network -- may reboot every eero, like the confirmed DNS \
             write path",
        );
        self.transport
            .resource(
                &routes::dhcp::DHCP_SET_CONNECTION_MODE,
                network_id,
                parent,
                &[],
                RequestBody::Json(json!({ "connection": { "mode": mode } })),
            )
            .await
    }

    /// Enables or disables NAT port randomization — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dhcp.py:244-290`
    /// (`DhcpAPI.set_nat_port_randomization`). Issues a JSON `PUT` to
    /// [`crate::routes::dhcp::DHCP_SET_NAT_PORT_RANDOMIZATION`] with
    /// `{"nat_port_randomization": enabled}`. Logs
    /// [`crate::links::warn_uncharacterised_write`] with `"set NAT port randomization for
    /// network -- may reboot every eero, like the confirmed DNS write path"` immediately before
    /// issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_nat_port_randomization(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write(
            "set NAT port randomization for network -- may reboot every eero, like the \
             confirmed DNS write path",
        );
        self.transport
            .resource(
                &routes::dhcp::DHCP_SET_NAT_PORT_RANDOMIZATION,
                network_id,
                parent,
                &[],
                RequestBody::Json(json!({ "nat_port_randomization": enabled })),
            )
            .await
    }

    /// Encrypts `PPPoE` credentials for an eero — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dhcp.py:292-336` (`DhcpAPI.set_pppoe`). Issues a JSON
    /// `POST` to [`crate::routes::dhcp::DHCP_SET_PPPOE`] with `{"pppoe": {"username": ...,
    /// "password": ...}}`. `eero_serial_or_id` addresses the eero directly — there is no
    /// `network_id` parameter. The response carries an encrypted credential blob under an
    /// observed-only key (`data.pppoe_credentials` in Python's own test fixture, not declared in
    /// the module's docstring); this method does not interpret it. `password` is never logged.
    /// Logs [`crate::links::warn_uncharacterised_write`] (`"encrypt PPPoE credentials for
    /// eero"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_pppoe(
        &self,
        eero_serial_or_id: &str,
        username: &str,
        password: &str,
    ) -> Result<Envelope, Error> {
        let url = routes::dhcp::DHCP_SET_PPPOE.resolve(
            self.transport.api_host(),
            eero_serial_or_id,
            None,
        )?;
        links::warn_uncharacterised_write("encrypt PPPoE credentials for eero");
        self.transport
            .request(
                routes::dhcp::DHCP_SET_PPPOE.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({
                    "pppoe": { "username": username, "password": password }
                })),
            )
            .await
    }
}

/// Copies only the caller-supplied keys that the API declares for a field.
///
/// Ported from `_filter_given_keys` (`eero-api src/eero/api/dhcp.py:59-79`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "field"` if `mapping` carries a key outside
/// `allowed`.
fn filter_given_keys(
    mapping: &Map<String, Value>,
    allowed: &[&str],
) -> Result<Map<String, Value>, Error> {
    let unknown: Vec<&str> = mapping
        .keys()
        .map(String::as_str)
        .filter(|key| !allowed.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err(Error::validation(
            "field",
            format!(
                "unrecognised field(s): {}; expected one of {}",
                py_list(&unknown),
                py_list(allowed)
            ),
        ));
    }
    Ok(mapping.clone())
}
