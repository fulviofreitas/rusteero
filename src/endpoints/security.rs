//! Security settings API: the read-only (`GET`) half of `eero-api`'s `SecurityAPI`.
//!
//! Ported from `eero-api src/eero/api/security.py`. This phase (3, GET-only) covers
//! `SecurityAPI.get_security_settings` only.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

use super::networks::put_network_settings;

/// The read-only half of `eero-api`'s `SecurityAPI` (`src/eero/api/security.py`).
///
/// Build one with [`SecurityApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `SecurityApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct SecurityApi {
    transport: Arc<Transport>,
}

impl SecurityApi {
    /// Wraps `transport` as a `SecurityApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets security settings for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:36-57`
    /// (`SecurityAPI.get_security_settings`). Sends `GET`
    /// [`crate::routes::GET_SECURITY_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated security sub-resource; there is no such
    /// sub-resource on the wire. The caller is expected to read the relevant keys (`wpa3`,
    /// `band_steering`, `upnp`, `ipv6_upstream`, `ipv6_downstream`, `thread`, and any others the
    /// server includes) out of the returned envelope's `data`, exactly as Python's own docstring
    /// instructs. This method never extracts, renames or reshapes any field — doing so would
    /// transform the raw payload the rest of this crate promises never to touch.
    pub async fn get_security_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_SECURITY_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable WPA3 encryption.
    ///
    /// Ported from `SecurityAPI.set_wpa3` (`security.py:59-90`). Sends `{"wpa3": enabled}`
    /// (`security.py:89`) through `put_network_settings`, the call site this crate shares across
    /// `NetworksApi`, `DnsApi`, `SecurityApi` and `SqmApi` for this one resource.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_wpa3(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        put_network_settings(&self.transport, network_id, json!({ "wpa3": enabled })).await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable band steering.
    ///
    /// Ported from `SecurityAPI.set_band_steering` (`security.py:92-123`). Sends
    /// `{"band_steering": enabled}` (`security.py:122`) through `put_network_settings`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_band_steering(
        &self,
        network_id: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "band_steering": enabled }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable `UPnP`.
    ///
    /// Ported from `SecurityAPI.set_upnp` (`security.py:125-156`). Sends `{"upnp": enabled}`
    /// (`security.py:155`) through `put_network_settings`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_upnp(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        put_network_settings(&self.transport, network_id, json!({ "upnp": enabled })).await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable IPv6.
    ///
    /// Ported from `SecurityAPI.set_ipv6` (`security.py:158-189`). Sends **both**
    /// `{"ipv6_upstream": enabled, "ipv6_downstream": enabled}` (`security.py:185-188`) through
    /// `put_network_settings` — a single flag fans out to two wire keys. Contrast with
    /// `DnsApi::set_ipv6_dns`, which sets only `ipv6_upstream`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_ipv6(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "ipv6_upstream": enabled, "ipv6_downstream": enabled }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable Thread.
    ///
    /// Ported from `SecurityAPI.set_thread` (`security.py:191-222`). Sends `{"thread": enabled}`
    /// (`security.py:221`) through `put_network_settings`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_thread(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        put_network_settings(&self.transport, network_id, json!({ "thread": enabled })).await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — configure multiple security settings in one
    /// call.
    ///
    /// Ported from `SecurityAPI.configure_security` (`security.py:224-282`). Builds a body from
    /// only the arguments actually supplied — `{"wpa3": ...}`, `{"band_steering": ...}` and
    /// `{"upnp": ...}` only when their argument is `Some` (`security.py:256-263`),
    /// `{"ipv6_upstream": ..., "ipv6_downstream": ...}` both set from `ipv6` when it is `Some`
    /// (`security.py:265-267`, the same one-flag-two-keys fan-out as `set_ipv6` above), and
    /// `{"thread": ...}` only when `thread` is `Some` (`security.py:269-270`) — then sends the
    /// merged body through `put_network_settings`. No key is ever sent as `null` for an omitted
    /// argument.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if every argument is `None`, before any request is sent. This
    /// is a deliberate divergence from Python, which never contacts the server in that case
    /// either but instead fabricates a local `{"meta": {"code": 400}, "data": {}}` response
    /// (`security.py:272-274`) — a response that never actually came from the wire. Inventing a
    /// fake envelope here would violate this crate's raw-payload contract more than simply
    /// refusing before any request is built (port plan §3.3). Returns
    /// `Error::Authentication("Not authenticated")` if no valid session is configured, or
    /// whatever other status-mapped error the request produces otherwise — see
    /// `Transport::send`.
    pub async fn configure_security(
        &self,
        network_id: &str,
        wpa3: Option<bool>,
        band_steering: Option<bool>,
        upnp: Option<bool>,
        ipv6: Option<bool>,
        thread: Option<bool>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        if let Some(wpa3) = wpa3 {
            payload.insert("wpa3".to_owned(), Value::Bool(wpa3));
        }
        if let Some(band_steering) = band_steering {
            payload.insert("band_steering".to_owned(), Value::Bool(band_steering));
        }
        if let Some(upnp) = upnp {
            payload.insert("upnp".to_owned(), Value::Bool(upnp));
        }
        if let Some(ipv6) = ipv6 {
            payload.insert("ipv6_upstream".to_owned(), Value::Bool(ipv6));
            payload.insert("ipv6_downstream".to_owned(), Value::Bool(ipv6));
        }
        if let Some(thread) = thread {
            payload.insert("thread".to_owned(), Value::Bool(thread));
        }
        if payload.is_empty() {
            return Err(Error::Validation {
                field: "wpa3, band_steering, upnp, ipv6, thread".to_owned(),
                message: "at least one security setting must be provided".to_owned(),
            });
        }

        put_network_settings(&self.transport, network_id, Value::Object(payload)).await
    }
}
