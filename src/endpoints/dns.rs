//! DNS settings API: `eero-api`'s `DnsAPI`, v8.0.4.
//!
//! Ported from `eero-api src/eero/api/dns.py` at v8.0.4 (module docstring, `dns.py:1-45`). Wire
//! format (live-verified 2026-09-12 against API 2.2):
//!
//! ```text
//! PUT networks/{id}/settings
//!
//! IPv4:  {"dns":  {"mode": "custom"|"automatic",
//!                  "custom": {"ips": [...]}}}
//! IPv6:  {"ipv6": {"name_servers": {"mode": "custom"|"automatic",
//!                                   "custom": [...]}}}
//! ```
//!
//! The two address families are **independent** objects with their own mode selectors, and their
//! shapes are **asymmetric**: the IPv4 list nests under `custom.ips` while the IPv6 list sits
//! directly under `custom`. This is not refactored into a shared helper that assumes symmetry.
//!
//! **A DNS change reboots the whole mesh** (live-verified 2026-09-12, `dns.py`'s module
//! docstring) — every write in this module calls
//! [`crate::links::warn_uncharacterised_write`] with the identical operation string
//! `"write DNS settings for network — reboots every eero"` immediately before the request.
//!
//! Historical note: releases v4.1.3 through v6.2.0 sent a flat `custom_dns` field (and
//! `dns_caching`); neither field exists on the API, which accepts unrecognised keys with HTTP 200
//! and silently discards them (issue #123) — the entire pre-v7.0.0 `dns.rs` body this file
//! replaces was dead code that reported success while changing nothing server-side.

use std::net::IpAddr;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// Maximum custom DNS servers accepted per address family.
///
/// Ported from `MAX_DNS_SERVERS_PER_FAMILY` (`dns.py:69`). Mirrors the eero app, which exposes
/// exactly two slots per family (primary + secondary). This is a deliberate client-side safety
/// limit, **not** an API-enforced one — the API was observed accepting more in a single family
/// during probing, which coincided with a network outage whose mechanism was never established
/// (`dns.py:60-68`).
pub const MAX_DNS_SERVERS_PER_FAMILY: usize = 2;

/// `DnsAPI` (`src/eero/api/dns.py`), v8.0.4.
///
/// Build one with [`DnsApi::new`], wrapping a [`Transport`] shared with the rest of the
/// [`crate::api::EeroApi`] aggregator — `DnsApi` never constructs or owns a `Transport` itself.
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
    /// Ported from `eero-api src/eero/api/dns.py:222-249` (`DnsAPI.get_dns_settings`). Sends
    /// `GET` [`crate::routes::dns::GET_DNS_SETTINGS`] — DNS settings are part of the network
    /// resource; there is no dedicated DNS sub-resource on the wire. This method takes no
    /// `parent`: Python's own `get_dns_settings` has no `parent` keyword at all (unlike every
    /// other read in this crate's DNS/security/SQM/thread/wpa3 group), resolving unconditionally
    /// via a bare-id template.
    ///
    /// Relevant response paths, per Python's own docstring: `data.dns.mode` (`"custom"` or
    /// `"automatic"`, the IPv4 selector), `data.dns.custom.ips` (configured IPv4 servers,
    /// retained even when the mode is `"automatic"`), `data.dns.parent.ips` (the ISP-provided
    /// upstream resolvers), `data.dns.caching`, `data.ipv6.name_servers.mode` (the IPv6
    /// selector), and `data.ipv6.name_servers.custom` (configured IPv6 servers, stored fully
    /// expanded, e.g. `"2606:4700:4700:0:0:0:0:1111"`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_dns_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::dns::GET_DNS_SETTINGS,
                network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `PUT /2.2/networks/{id}/settings` — enables or disables DNS caching.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:253-280` (`DnsAPI.set_dns_caching`). Sends the
    /// **nested** `{"dns": {"caching": enabled}}` (`dns.py:281`) — not the dead, flat
    /// `dns_caching` field v4.1.3-v6.2.0 sent (issue #123).
    ///
    /// # Errors
    ///
    /// See [`DnsApi::get_dns_settings`].
    pub async fn set_dns_caching(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_settings(network_id, json!({ "dns": { "caching": enabled } }), parent)
            .await
    }

    /// Sets custom DNS servers from a mixed IPv4/IPv6 list — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:282-334` (`DnsAPI.set_custom_dns`). The list may
    /// contain IPv4 and/or IPv6 literals; each represented family is written to its own field and
    /// switched to custom mode — `{"dns": {"mode": "custom", "custom": {"ips": ipv4}}}` and/or
    /// `{"ipv6": {"name_servers": {"mode": "custom", "custom": ipv6}}}` (`dns.py:326-333`). **A
    /// family not represented in the list is left untouched entirely** — an IPv4-only list does
    /// not alter IPv6, and vice versa. To address a single family explicitly, prefer
    /// [`DnsApi::set_custom_dns_ipv4`]/[`DnsApi::set_custom_dns_ipv6`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` before any request is sent if an entry is not a valid,
    /// correctly-versioned IP literal (see this module's private `validate_family_servers` helper
    /// for the validation rules and their divergences from Python), if a family exceeds
    /// [`MAX_DNS_SERVERS_PER_FAMILY`], or if `dns_servers` yields no server in either family
    /// (`dns.py:311-314`'s `"no servers supplied; use clear_custom_dns() to switch to automatic
    /// DNS"`). Otherwise see [`DnsApi::get_dns_settings`].
    pub async fn set_custom_dns(
        &self,
        network_id: &str,
        dns_servers: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let (ipv4, ipv6) = split_by_family(dns_servers, "dns_servers")?;
        if ipv4.is_empty() && ipv6.is_empty() {
            return Err(Error::validation(
                "dns_servers",
                "no servers supplied; use clear_custom_dns() to switch to automatic DNS",
            ));
        }

        let mut payload = serde_json::Map::new();
        if !ipv4.is_empty() {
            payload.insert(
                "dns".to_owned(),
                json!({ "mode": DNS_MODE_CUSTOM, "custom": { "ips": ipv4 } }),
            );
        }
        if !ipv6.is_empty() {
            payload.insert(
                "ipv6".to_owned(),
                json!({ "name_servers": { "mode": DNS_MODE_CUSTOM, "custom": ipv6 } }),
            );
        }
        self.put_settings(network_id, Value::Object(payload), parent)
            .await
    }

    /// Sets the IPv4 custom DNS servers, leaving IPv6 untouched — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:336-369` (`DnsAPI.set_custom_dns_ipv4`). Sends
    /// `{"dns": {"mode": "custom", "custom": {"ips": servers}}}` (`dns.py:367-369`); never
    /// touches `ipv6`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` for the same reasons as [`DnsApi::set_custom_dns`]'s IPv4
    /// half, plus for an empty `dns_servers` (`dns.py:359-362`'s `"no servers supplied; use
    /// clear_custom_dns(family='ipv4') to switch to automatic"`). Otherwise see
    /// [`DnsApi::get_dns_settings`].
    pub async fn set_custom_dns_ipv4(
        &self,
        network_id: &str,
        dns_servers: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let servers = validate_family_servers(dns_servers, 4, "dns_servers")?;
        if servers.is_empty() {
            return Err(Error::validation(
                "dns_servers",
                "no servers supplied; use clear_custom_dns(family='ipv4') to switch to automatic",
            ));
        }
        self.put_settings(
            network_id,
            json!({ "dns": { "mode": DNS_MODE_CUSTOM, "custom": { "ips": servers } } }),
            parent,
        )
        .await
    }

    /// Sets the IPv6 custom DNS servers, leaving IPv4 untouched — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:371-409` (`DnsAPI.set_custom_dns_ipv6`). Sends
    /// `{"ipv6": {"name_servers": {"mode": "custom", "custom": servers}}}` (`dns.py:406-408`);
    /// never touches `dns`. The API stores IPv6 addresses fully expanded, so a value written
    /// compressed (e.g. `"2606:4700:4700::1111"`) reads back expanded
    /// (`"2606:4700:4700:0:0:0:0:1111"`) — compare via parsed-address equality, never string
    /// equality (`dns.py:378-380`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` for the same reasons as [`DnsApi::set_custom_dns`]'s IPv6
    /// half, plus for an empty `dns_servers` (`dns.py:398-401`'s `"no servers supplied; use
    /// clear_custom_dns(family='ipv6') to switch to automatic"`). Otherwise see
    /// [`DnsApi::get_dns_settings`].
    pub async fn set_custom_dns_ipv6(
        &self,
        network_id: &str,
        dns_servers: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let servers = validate_family_servers(dns_servers, 6, "dns_servers")?;
        if servers.is_empty() {
            return Err(Error::validation(
                "dns_servers",
                "no servers supplied; use clear_custom_dns(family='ipv6') to switch to automatic",
            ));
        }
        self.put_settings(
            network_id,
            json!({ "ipv6": { "name_servers": { "mode": DNS_MODE_CUSTOM, "custom": servers } } }),
            parent,
        )
        .await
    }

    /// Switches DNS back to automatic — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:411-454` (`DnsAPI.clear_custom_dns`). **This is
    /// non-destructive**: the API retains the configured servers rather than discarding them
    /// (live-verified 2026-09-12, mirroring the eero app's "ISP DNS (Default)" option) — the
    /// payload never carries a `custom` key, only a `mode` selector: `family in (None, "ipv4")`
    /// sends `{"dns": {"mode": "automatic"}}`; `family in (None, "ipv6")` sends `{"ipv6":
    /// {"name_servers": {"mode": "automatic"}}}` (`dns.py:445-450`). To re-enable the stored
    /// servers, call [`DnsApi::set_dns_mode`] with `mode = "custom"` and `custom_servers = None`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "family", .. }` if `family` is not `None`, `"ipv4"`,
    /// or `"ipv6"` (`dns.py:441-442`). Otherwise see [`DnsApi::get_dns_settings`].
    pub async fn clear_custom_dns(
        &self,
        network_id: &str,
        family: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if !matches!(family, None | Some("ipv4" | "ipv6")) {
            return Err(Error::validation(
                "family",
                "must be 'ipv4', 'ipv6', or None",
            ));
        }
        let mut payload = serde_json::Map::new();
        if matches!(family, None | Some("ipv4")) {
            payload.insert("dns".to_owned(), json!({ "mode": DNS_MODE_AUTOMATIC }));
        }
        if matches!(family, None | Some("ipv6")) {
            payload.insert(
                "ipv6".to_owned(),
                json!({ "name_servers": { "mode": DNS_MODE_AUTOMATIC } }),
            );
        }
        self.put_settings(network_id, Value::Object(payload), parent)
            .await
    }

    /// Sets DNS mode for the network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:456-524` (`DnsAPI.set_dns_mode`). `mode` is
    /// trimmed and lower-cased before matching. `"auto"`/`"automatic"` delegates to
    /// [`DnsApi::clear_custom_dns`] with `family = None` (`dns.py:503-504`) — this is why
    /// `"automatic"` retains servers: it sends the exact same mode-only payload
    /// `clear_custom_dns` does. `"custom"` with a non-empty `custom_servers` delegates to
    /// [`DnsApi::set_custom_dns`] (`dns.py:507-508`). `"custom"` with no servers **re-enables the
    /// servers already stored on the network** — `{"dns": {"mode": "custom"}, "ipv6":
    /// {"name_servers": {"mode": "custom"}}}`, no `custom` key on either (`dns.py:510-518`) —
    /// this is the exact inverse of `clear_custom_dns`, and is a live-verified server-side
    /// property, not a client-side merge (g5 brief §3.1).
    ///
    /// Named provider presets (`"cloudflare"`, `"google"`, `"opendns"`, ...) are **deliberately
    /// not offered** — removed upstream in v7.0.0. The API serves its own provider catalogue at
    /// `data.dns.default_test_servers` (read via [`DnsApi::get_dns_settings`]); a hardcoded copy
    /// would duplicate server-owned data and go stale (`dns.py:487-495`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` for any `mode` that is not
    /// `"auto"`/`"automatic"`/`"custom"` (case-insensitive, surrounding whitespace ignored),
    /// including every provider-preset name (`dns.py:520-524`) — no request is sent in this case,
    /// matching `dns.py` itself, which also never contacts the server for an unrecognised mode.
    /// Otherwise see [`DnsApi::set_custom_dns`]/[`DnsApi::clear_custom_dns`]/
    /// [`DnsApi::get_dns_settings`].
    pub async fn set_dns_mode(
        &self,
        network_id: &str,
        mode: &str,
        custom_servers: Option<&[&str]>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let normalized = mode.trim().to_lowercase();
        match normalized.as_str() {
            "auto" | DNS_MODE_AUTOMATIC => self.clear_custom_dns(network_id, None, parent).await,
            DNS_MODE_CUSTOM => match custom_servers {
                Some(servers) if !servers.is_empty() => {
                    self.set_custom_dns(network_id, servers, parent).await
                }
                _ => {
                    self.put_settings(
                        network_id,
                        json!({
                            "dns": { "mode": DNS_MODE_CUSTOM },
                            "ipv6": { "name_servers": { "mode": DNS_MODE_CUSTOM } },
                        }),
                        parent,
                    )
                    .await
                }
            },
            _ => Err(Error::validation(
                "mode",
                format!(
                    "{mode:?} is not a valid DNS mode; expected 'auto' or 'custom'. Provider \
                     presets are available from the API at data.dns.default_test_servers — pass \
                     those addresses as custom_servers"
                ),
            )),
        }
    }

    /// Shared `PUT /2.2/networks/{id}/settings` call site behind every writer above.
    ///
    /// Ported from `eero-api src/eero/api/dns.py:183-220` (`DnsAPI._put_settings`): every DNS
    /// write funnels through here, so the reboot warning lives in exactly one place. Logs the
    /// identical operation string Python does (`dns.py:206`, em dash) via
    /// [`warn_uncharacterised_write`] immediately before the request, per this crate's own
    /// conventions.
    async fn put_settings(
        &self,
        network_id: &str,
        payload: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        warn_uncharacterised_write("write DNS settings for network — reboots every eero");
        self.transport
            .resource(
                &routes::dns::DNS_PUT_SETTINGS,
                network_id,
                parent,
                &[],
                RequestBody::Json(payload),
            )
            .await
    }
}

/// DNS mode selector value meaning "use the caller-supplied server list".
///
/// Ported from `DNS_MODE_CUSTOM` (`dns.py:72`).
const DNS_MODE_CUSTOM: &str = "custom";

/// DNS mode selector value meaning "use the network's automatically-assigned servers".
///
/// Ported from `DNS_MODE_AUTOMATIC` (`dns.py:73`).
const DNS_MODE_AUTOMATIC: &str = "automatic";

/// Partitions a mixed list of IP literals into validated, normalised IPv4 and IPv6 groups, each
/// capped at [`MAX_DNS_SERVERS_PER_FAMILY`].
///
/// Ported from `_split_by_family` (`dns.py:139-165`) composed with `_validate_servers`
/// (`dns.py:80-137`) — Python validates parseability once while bucketing by family, then
/// re-validates (including the per-family cap) inside each of `set_custom_dns`'s two
/// `_validate_servers` calls; this port folds both passes into one, with the cap applied to each
/// bucket after it is fully collected, so the same test-visible ordering holds: every entry's
/// basic IP-literal validity is checked (raising on the first bad one, in input order) before
/// either family's length is checked against the cap.
///
/// # Errors
///
/// Returns `Error::Validation` with `field` as `field` for the same reasons
/// [`validate_family_servers`] can — see that function's doc comment — for whichever entry (or
/// family) triggers it first.
fn split_by_family(servers: &[&str], field: &str) -> Result<(Vec<String>, Vec<String>), Error> {
    let mut ipv4_raw = Vec::new();
    let mut ipv6_raw = Vec::new();
    for &entry in servers {
        let (family, _) = parse_one_server(entry, field)?;
        match family {
            4 => ipv4_raw.push(entry),
            _ => ipv6_raw.push(entry),
        }
    }
    let ipv4 = validate_family_servers(&ipv4_raw, 4, field)?;
    let ipv6 = validate_family_servers(&ipv6_raw, 6, field)?;
    Ok((ipv4, ipv6))
}

/// Validates and normalises a list of DNS server literals for exactly one address family.
///
/// Ported from `_validate_servers` (`dns.py:80-137`): rejects a list longer than
/// [`MAX_DNS_SERVERS_PER_FAMILY`] with a message naming the actual count (`dns.py:97-101`);
/// otherwise validates and normalises every entry via [`parse_one_server`], rejecting the first
/// one that does not parse as `family`.
///
/// A `Vec<&str>`/`&[&str]`-typed caller cannot pass a non-list or a non-string entry at all —
/// Python's own `isinstance(servers, list)`/`isinstance(entry, str)` guards (`dns.py:90-91,
/// 105-108`) have no Rust equivalent to port because the type system already makes both
/// unrepresentable; this is a divergence in test surface, not in observable behaviour for any
/// value that *does* compile.
///
/// # Errors
///
/// Returns `Error::Validation` with `field` as `field`: for more than
/// [`MAX_DNS_SERVERS_PER_FAMILY`] entries; for an entry that, once trimmed, is empty; for an
/// entry containing a `%` zone identifier (`dns.py:112-116`, meaningless to a cloud API — Rust's
/// own `IpAddr::from_str` already rejects every zone-scoped literal on stable, but this explicit
/// check keeps the same field/message shape Python raises rather than a generic parse failure,
/// per g5 brief §6 note 7); for an entry that does not parse as a valid IP literal at all; or for
/// an entry that parses but is the wrong address family. Leading zero octets (e.g.
/// `"010.0.0.1"`) are rejected identically by both ecosystems' parsers — no divergence there (g5
/// brief §6 note 5). Surrounding whitespace is stripped before every other check, matching
/// Python's explicit `entry.strip()` (`dns.py:112-113`) — `IpAddr::from_str` has no equivalent
/// leniency of its own (g5 brief §6 note 6).
fn validate_family_servers(
    servers: &[&str],
    family: u8,
    field: &str,
) -> Result<Vec<String>, Error> {
    if servers.len() > MAX_DNS_SERVERS_PER_FAMILY {
        return Err(Error::validation(
            field,
            format!(
                "at most {MAX_DNS_SERVERS_PER_FAMILY} IPv{family} servers are supported (got {}); \
                 this matches the eero app's primary/secondary slots",
                servers.len()
            ),
        ));
    }
    let mut normalized = Vec::with_capacity(servers.len());
    for &entry in servers {
        let (got_family, address) = parse_one_server(entry, field)?;
        if got_family != family {
            return Err(Error::validation(
                field,
                format!("{entry:?} is an IPv{got_family} address, expected IPv{family}"),
            ));
        }
        normalized.push(address);
    }
    Ok(normalized)
}

/// Validates, and normalises to its canonical string form, one DNS server literal.
///
/// Trims surrounding whitespace, rejects an empty result, rejects a `%`-scoped (zone identifier)
/// literal, then parses via [`std::net::IpAddr::from_str`]. Returns the parsed address's IP
/// version (`4` or `6`) alongside its canonical `Display` form — for IPv6 this is the
/// RFC 5952 compressed form, matching Python's own `str(ipaddress.ip_address(...))`
/// normalisation (confirmed live 2026-09-12: a value written compressed reads back fully
/// expanded from the *server*, but what this crate sends on the wire is always compressed,
/// exactly like Python).
///
/// See g5 brief §6 note 4 for one known residual divergence this function does **not** attempt
/// to reconcile: Python's `ipaddress` module renders an IPv4-mapped IPv6 address (e.g.
/// `"::ffff:192.168.1.1"`) as hex groups (`"::ffff:c0a8:101"`), while Rust's
/// `std::net::Ipv6Addr::fmt` special-cases the same address family and renders it in dotted-quad
/// form (`"::ffff:192.168.1.1"`) instead. No live capture has exercised an IPv4-mapped literal
/// against the real API as of this port, so which wire form the server actually expects is
/// unconfirmed; this is flagged, not silently "fixed" in either direction.
fn parse_one_server(entry: &str, field: &str) -> Result<(u8, String), Error> {
    let candidate = entry.trim();
    if candidate.is_empty() {
        return Err(Error::validation(field, "IP address must not be empty"));
    }
    if candidate.contains('%') {
        return Err(Error::validation(
            field,
            format!("{entry:?} has a zone identifier, which is not valid for a DNS server"),
        ));
    }
    let address: IpAddr = candidate
        .parse()
        .map_err(|_| Error::validation(field, format!("{entry:?} is not a valid IP address")))?;
    let family = if address.is_ipv4() { 4 } else { 6 };
    Ok((family, address.to_string()))
}
