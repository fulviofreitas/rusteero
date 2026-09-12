//! SQM/QoS settings API: the read-only (`GET`) half of `eero-api`'s `SqmAPI`.
//!
//! Ported from `eero-api src/eero/api/sqm.py`. This phase (3, GET-only) covers
//! `SqmAPI.get_sqm_settings` only.
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

/// The read-only half of `eero-api`'s `SqmAPI` (`src/eero/api/sqm.py`).
///
/// Build one with [`SqmApi::new`], wrapping a [`Transport`] already shared with the rest of the
/// (not-yet-built) `EeroApi` aggregator — `SqmApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct SqmApi {
    transport: Arc<Transport>,
}

impl SqmApi {
    /// Wraps `transport` as a `SqmApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets SQM (Smart Queue Management) / `QoS` settings for a network — returns the raw Eero
    /// API response.
    ///
    /// Ported from `eero-api src/eero/api/sqm.py:36-57` (`SqmAPI.get_sqm_settings`). Sends `GET`
    /// [`crate::routes::GET_SQM_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated SQM sub-resource; there is no such sub-resource
    /// on the wire. The caller is expected to read the relevant keys (`sqm`, and any others the
    /// server includes) out of the returned envelope's `data`, exactly as Python's own docstring
    /// instructs. This method never extracts, renames or reshapes any field — doing so would
    /// transform the raw payload the rest of this crate promises never to touch.
    pub async fn get_sqm_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_SQM_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable SQM (Smart Queue
    /// Management).
    ///
    /// Ported from `SqmAPI.set_sqm_enabled` (`sqm.py:59-87`). Sends a **flat** `{"sqm": enabled}`
    /// body (`sqm.py:86`) through `put_network_settings` — unlike `set_sqm_bandwidth`,
    /// `configure_sqm` and `set_sqm_auto` below, which all nest an object under `"sqm"` instead
    /// of a bare bool.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_sqm_enabled(
        &self,
        network_id: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        put_network_settings(&self.transport, network_id, json!({ "sqm": enabled })).await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — set SQM upload/download bandwidth limits.
    ///
    /// Ported from `SqmAPI.set_sqm_bandwidth` (`sqm.py:89-135`). Sends a **nested**
    /// `{"sqm": {"enabled": true, "upload_bandwidth"?, "download_bandwidth"?}}` body
    /// (`sqm.py:113-119,131-135`) through `put_network_settings` — `upload_bandwidth`/
    /// `download_bandwidth` are included only when `upload_mbps`/`download_mbps` are `Some`.
    ///
    /// Unverified upstream: Python leaves a `TODO: Verify` comment at `eero-api src/eero/api/
    /// sqm.py:128-130` questioning whether this payload should be flattened (e.g. `{"sqm": true,
    /// "upload_bandwidth": N}`) rather than nested, as it is here — nobody has confirmed either
    /// shape against a real device. This port implements exactly what Python sends today, nested;
    /// do not "fix" this without a live capture, and mark the corresponding `PARITY.md` row
    /// "ported — shape unverified upstream", not "ported".
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_sqm_bandwidth(
        &self,
        network_id: &str,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
    ) -> Result<Envelope, Error> {
        let mut sqm = Map::new();
        sqm.insert("enabled".to_owned(), Value::Bool(true));
        if let Some(upload_mbps) = upload_mbps {
            sqm.insert("upload_bandwidth".to_owned(), Value::from(upload_mbps));
        }
        if let Some(download_mbps) = download_mbps {
            sqm.insert("download_bandwidth".to_owned(), Value::from(download_mbps));
        }
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "sqm": Value::Object(sqm) }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — configure SQM (enable/disable plus optional
    /// bandwidth limits) in one call.
    ///
    /// Ported from `SqmAPI.configure_sqm` (`sqm.py:137-180`). Sends a **nested**
    /// `{"sqm": {"enabled": enabled, "upload_bandwidth"?, "download_bandwidth"?}}` body
    /// (`sqm.py:163-179`) through `put_network_settings` — the bandwidth keys are included only
    /// when `enabled` is `true` *and* the corresponding argument is `Some` (`sqm.py:165-169`); if
    /// `enabled` is `false` neither bandwidth key is ever sent, regardless of the arguments.
    ///
    /// Unverified upstream: same caveat as `set_sqm_bandwidth` above, at `eero-api src/eero/api/
    /// sqm.py:173-175` — Python's `TODO: Verify` questions whether this combined payload should
    /// be flattened rather than nested. This port implements exactly what Python sends today,
    /// nested; do not "fix" this without a live capture, and mark the corresponding `PARITY.md`
    /// row "ported — shape unverified upstream", not "ported".
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn configure_sqm(
        &self,
        network_id: &str,
        enabled: bool,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
    ) -> Result<Envelope, Error> {
        let mut sqm = Map::new();
        sqm.insert("enabled".to_owned(), Value::Bool(enabled));
        if enabled {
            if let Some(upload_mbps) = upload_mbps {
                sqm.insert("upload_bandwidth".to_owned(), Value::from(upload_mbps));
            }
            if let Some(download_mbps) = download_mbps {
                sqm.insert("download_bandwidth".to_owned(), Value::from(download_mbps));
            }
        }
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "sqm": Value::Object(sqm) }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — set SQM to automatic mode (auto-detect
    /// bandwidth).
    ///
    /// Ported from `SqmAPI.set_sqm_auto` (`sqm.py:182-204`). Takes no bool argument, unlike the
    /// other three setters above — Python always enables SQM in auto mode. Sends the fixed,
    /// **nested** body `{"sqm": {"enabled": true, "mode": "auto"}}` (`sqm.py:203`) through
    /// `put_network_settings`.
    ///
    /// Unverified upstream: same caveat as `set_sqm_bandwidth`/`configure_sqm` above, a third
    /// time, at `eero-api src/eero/api/sqm.py:197-199` — Python's `TODO: Verify` questions
    /// whether this payload should be flattened (e.g. `{"sqm": true, "mode": "auto"}`) rather
    /// than nested. This port implements exactly what Python sends today, nested; do not "fix"
    /// this without a live capture, and mark the corresponding `PARITY.md` row "ported — shape
    /// unverified upstream", not "ported".
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_sqm_auto(&self, network_id: &str) -> Result<Envelope, Error> {
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "sqm": { "enabled": true, "mode": "auto" } }),
        )
        .await
    }
}
