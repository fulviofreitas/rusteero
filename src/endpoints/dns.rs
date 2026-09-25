//! DNS settings API: the read-only (`GET`) half of `eero-api`'s `DnsAPI`.
//!
//! Ported from `eero-api src/eero/api/dns.py`. This phase (3, GET-only) covers
//! `DnsAPI.get_dns_settings` only.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

use super::networks::put_network_settings;

/// The read-only half of `eero-api`'s `DnsAPI` (`src/eero/api/dns.py`).
///
/// Build one with [`DnsApi::new`], wrapping a [`Transport`] already shared with the rest of the
/// (not-yet-built) `EeroApi` aggregator — `DnsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct DnsApi {
    transport: Arc<Transport>,
}

impl DnsApi {
    /// Wraps `transport` as a `DnsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets DNS configuration for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:36-57` (`DnsAPI.get_dns_settings`). Sends `GET`
    /// [`crate::routes::GET_DNS_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated DNS sub-resource; there is no such sub-resource
    /// on the wire. The caller is expected to read the relevant keys (`dns_caching`,
    /// `custom_dns`, `ipv6_upstream`, and any others the server includes) out of the returned
    /// envelope's `data`, exactly as Python's own docstring instructs. This method never
    /// extracts, renames or reshapes any field — doing so would transform the raw payload the
    /// rest of this crate promises never to touch.
    pub async fn get_dns_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_DNS_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable DNS caching.
    ///
    /// Ported from `DnsAPI.set_dns_caching` (`dns.py:59-87`). Sends `{"dns_caching": enabled}`
    /// (`dns.py:86`) through `put_network_settings`, the call site this crate shares across
    /// `NetworksApi`, `DnsApi`, `SecurityApi` and `SqmApi` for this one resource.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_dns_caching(
        &self,
        network_id: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "dns_caching": enabled }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — set custom DNS servers.
    ///
    /// Ported from `DnsAPI.set_custom_dns` (`dns.py:89-124`). Sends `{"custom_dns": dns_servers}`
    /// (`dns.py:120-124`) through `put_network_settings`. `dns_servers` is **silently truncated
    /// to at most 2 entries** before the request body is built (`dns.py:114-116` — Python only
    /// logs a warning when it truncates; this port reproduces the truncation itself, not the
    /// logging, since no endpoint module in this crate logs at this layer — see the module docs
    /// for why). A 3rd-and-later entry is dropped entirely, never sent.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_custom_dns(
        &self,
        network_id: &str,
        dns_servers: &[&str],
    ) -> Result<Envelope, Error> {
        let truncated = &dns_servers[..dns_servers.len().min(2)];
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "custom_dns": truncated }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — clear custom DNS servers (revert to
    /// automatic DNS).
    ///
    /// Ported from `DnsAPI.clear_custom_dns` (`dns.py:126-135`), which delegates to
    /// `set_custom_dns([])` — this method does the same, delegating to
    /// `DnsApi::set_custom_dns` with an empty slice rather than duplicating its call site.
    ///
    /// # Errors
    ///
    /// See `DnsApi::set_custom_dns`.
    pub async fn clear_custom_dns(&self, network_id: &str) -> Result<Envelope, Error> {
        self.set_custom_dns(network_id, &[]).await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — set DNS mode to one of a handful of presets or
    /// a caller-supplied custom server list.
    ///
    /// Ported from `DnsAPI.set_dns_mode` (`dns.py:137-184`). `mode` selects the `custom_dns` body
    /// value sent through `put_network_settings`:
    ///
    /// | `mode` | body sent |
    /// |---|---|
    /// | `"cloudflare"` | `{"custom_dns": ["1.1.1.1", "1.0.0.1"]}` (`dns.py:165`) |
    /// | `"google"` | `{"custom_dns": ["8.8.8.8", "8.8.4.4"]}` (`dns.py:167`) |
    /// | `"opendns"` | `{"custom_dns": ["208.67.222.222", "208.67.220.220"]}` (`dns.py:169`) |
    /// | `"custom"` with a non-empty `custom_servers` | `{"custom_dns": custom_servers[..2]}` (`dns.py:171`) |
    /// | `"auto"` | `{"custom_dns": []}` (`dns.py:173`) |
    ///
    /// Any other `mode` — including `"custom"` with `custom_servers` absent or empty, which
    /// Python's own `mode == "custom" and custom_servers` truthiness check also rejects
    /// (`dns.py:170`) — is invalid.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` for an invalid `mode`, before any
    /// request is sent. This is a deliberate divergence from Python, which never contacts the
    /// server in that case either but instead fabricates a local
    /// `{"meta": {"code": 400}, "data": {}}` response (`dns.py:175-176`) — a response that never
    /// actually came from the wire. Inventing a fake envelope here would violate this crate's
    /// raw-payload contract more than simply refusing before any request is built (port plan
    /// §3.3). Returns `Error::Authentication("Not authenticated")` if no valid session is
    /// configured, or whatever other status-mapped error the request produces otherwise — see
    /// `Transport::send`.
    pub async fn set_dns_mode(
        &self,
        network_id: &str,
        mode: &str,
        custom_servers: Option<&[&str]>,
    ) -> Result<Envelope, Error> {
        let custom_dns: Vec<&str> = match mode {
            "cloudflare" => vec!["1.1.1.1", "1.0.0.1"],
            "google" => vec!["8.8.8.8", "8.8.4.4"],
            "opendns" => vec!["208.67.222.222", "208.67.220.220"],
            "custom" => match custom_servers {
                Some(servers) if !servers.is_empty() => servers[..servers.len().min(2)].to_vec(),
                _ => return Err(invalid_dns_mode(mode)),
            },
            "auto" => Vec::new(),
            _ => return Err(invalid_dns_mode(mode)),
        };
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "custom_dns": custom_dns }),
        )
        .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — enable or disable IPv6 DNS.
    ///
    /// Ported from `DnsAPI.set_ipv6_dns` (`dns.py:186-214`). Sends `{"ipv6_upstream": enabled}`
    /// (`dns.py:213`) through `put_network_settings`. Note this sets only the upstream flag —
    /// contrast with `SecurityApi::set_ipv6`, which sets both `ipv6_upstream` and
    /// `ipv6_downstream` from one bool.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_ipv6_dns(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        put_network_settings(
            &self.transport,
            network_id,
            json!({ "ipv6_upstream": enabled }),
        )
        .await
    }
}

/// Builds the `Error::Validation` returned by `DnsApi::set_dns_mode` for an unrecognised `mode`.
///
/// Private: this is a one-line helper factored out only because `set_dns_mode` has two identical
/// return sites for the same error (the catch-all arm and the `"custom"`-with-no-servers arm).
fn invalid_dns_mode(mode: &str) -> Error {
    Error::validation("mode", format!("unknown DNS mode: {mode}"))
}
