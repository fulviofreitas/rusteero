//! Wire constants: base URLs, size limits, timeouts, and other values ported from `eero-api`'s
//! `src/eero/const.py` at `v8.4.1`.
//!
//! ## Deliberate omissions
//!
//! - **`DEFAULT_HEADERS`**: `const.py`'s dict-literal form is not ported as a separate constant
//!   — [`DEFAULT_USER_AGENT`] and [`DEFAULT_ACCEPT_LANGUAGE`] are ported individually instead,
//!   since the request pipeline sends each of them as its own header (`api/base.py:136-183`).
//!   The `User-Agent` is genuinely sent on every request: `build_request_headers` sets
//!   `User-Agent: <DEFAULT_USER_AGENT>` unless the caller supplies their own, and
//!   [`crate::transport`] does the same (override via `TransportBuilder::user_agent`;
//!   [`LEGACY_USER_AGENT`] reproduces the previous default).
//! - **`EeroDeviceType`, `EeroNetworkStatus`, `EeroDeviceStatus`** (`const.py`'s trailing enums)
//!   are not ported. All three are dead code in the Python library — nothing in `src/eero/`
//!   imports or branches on them (confirmed by grep across the package) — so there is no
//!   observable serialization behaviour to preserve.

use std::time::Duration;

/// Scheme + host of the Eero cloud API, with no path, query, or trailing slash.
///
/// Ported from `API_HOST` (`const.py:7`). The single source of truth every versioned base URL
/// below (and [`api_endpoint`]) is derived from.
pub const API_HOST: &str = "https://api-user.e2ro.com";

/// Builds the base endpoint URL for a given API version segment (e.g. `"2.2"` or `"2.3"`).
///
/// Ported from `api_endpoint` (`const.py:10-25`): the single place that joins a version segment
/// onto [`API_HOST`]. `API_BASE_22`/`API_BASE_23` are this same join, pre-computed as `'static`
/// strings for the two versions this crate actually uses today; this function stays available
/// for call sites (e.g. `links`/`params`) that only have a version segment string, not an
/// `ApiVersion`.
#[must_use]
pub fn api_endpoint(version: &str) -> String {
    format!("{API_HOST}/{version}")
}

/// Default API version used by most families (networks, eeros, profiles, guest network, and the
/// majority of resources).
///
/// Ported from `API_VERSION_DEFAULT` (`const.py:30`).
pub const API_VERSION_DEFAULT: &str = "2.2";

/// API version required for device-mutation writes (nickname, pause).
///
/// Version 2.2 silently drops these writes: the server accepts the `PUT` and returns `200 OK`,
/// but the change never persists server-side. Version 2.3 processes the same writes correctly;
/// reads and every other resource stay on 2.2. Ported from `API_VERSION_DEVICE_WRITES`
/// (`const.py:36`); see `eero-api` issue #102.
pub const API_VERSION_DEVICE_WRITES: &str = "2.3";

/// API version the multi-static-IP family is served on.
///
/// Ported from `API_VERSION_MULTISTATICIP` (`const.py:39`).
pub const API_VERSION_MULTISTATICIP: &str = "2.3";

/// API version secondary-WAN configuration (network-level and per-device) is served on.
///
/// Ported from `API_VERSION_SECONDARY_WAN` (`const.py:43`).
pub const API_VERSION_SECONDARY_WAN: &str = "2.3";

/// Base URL for API version 2.2 — the default version for almost every endpoint.
///
/// Ported from `API_ENDPOINT` (`const.py:50`), i.e. `api_endpoint(API_VERSION_DEFAULT)`.
pub const API_BASE_22: &str = "https://api-user.e2ro.com/2.2";

/// Base URL for API version 2.3.
///
/// Ported from `DEVICE_UPDATE_ENDPOINT` (`const.py:51`), i.e.
/// `api_endpoint(API_VERSION_DEVICE_WRITES)`.
pub const API_BASE_23: &str = "https://api-user.e2ro.com/2.3";

/// Maximum number of bytes read from a single response body before the request is aborted.
///
/// Guards against unbounded memory consumption from a hostile or misbehaving upstream response.
/// Ported from `MAX_RESPONSE_BYTES` (`const.py:109`).
pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Overall request timeout, from connect through to the full response body.
///
/// Ported from `api/base.py`'s `aiohttp.ClientTimeout(total=30, sock_read=10)`; this is the
/// `total` half of that pair. See [`READ_TIMEOUT`] for the `sock_read` half. Unchanged at
/// `v8.0.4`.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-read timeout applied while streaming a response body.
///
/// Ported from `api/base.py`'s `sock_read=10`. Unchanged at `v8.0.4`.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Default time-to-live for cached responses.
///
/// Ported from `CACHE_TIMEOUT` (`const.py:106`).
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60);

/// Name of the cookie that carries the session token on an authenticated request, when the
/// legacy cookie is sent at all.
///
/// At `v8.0.4` the *primary* credential is the `X-User-Token` header ([`USER_TOKEN_HEADER`]); a
/// per-request `s=<token>` cookie is an optional secondary credential, attached only when
/// [`crate::transport::TransportBuilder::send_legacy_cookie`] is on (the default).
pub const SESSION_COOKIE_NAME: &str = "s";

/// Value of `meta.error` that signals the server wants the client to refresh its session and
/// retry the request exactly once.
///
/// Ported from the literal `"error.session.refresh"` inlined in `api/base.py`. Equivalent to
/// `errors::classify_error_code(..) == Some(ErrorGroup::SessionRefresh)`; kept as a `'static`
/// constant here since `transport.rs`'s refresh-signal sniff compares it directly.
pub const REFRESH_ERROR_CODE: &str = "error.session.refresh";

/// Wire key holding the session token in a `login`/`login/verify` response body.
///
/// Ported from the literal `"user_token"` used throughout `auth.py`'s login/verify handshake —
/// a *different* wire key from the one a refresh response would have carried before `v8.0.4`
/// removed that response-body-parsing step entirely (see the module docs' `// removed by phase
/// B` notes and [`crate::errors`] for the current, response-envelope-driven error model).
pub const USER_TOKEN_KEY: &str = "user_token";

/// Name of the header that carries the session token as the *primary* credential.
///
/// Ported from the literal `"X-User-Token"` (`api/base.py`). Attached, together with an optional
/// legacy `s=<token>` cookie, only when the resolved request URL's hostname and scheme both
/// exactly match the configured API host (see [`crate::transport`]'s credential-placement gate).
pub const USER_TOKEN_HEADER: &str = "X-User-Token";

/// Default value of the `User-Agent` header sent on every request.
///
/// Ported from `DEFAULT_USER_AGENT` (`const.py:64-96`). A mobile-style string, sent by
/// [`crate::transport`] on every request unless overridden via `TransportBuilder::user_agent`
/// (or `ClientBuilder::user_agent`).
///
/// The Eero cloud also reads this string as the client's app version and decides which
/// capabilities to advertise from it (`eero-api` issue #135). The same account, network and
/// session, one `GET networks/{id}` per string, counting the capabilities marked `capable`:
///
/// | Client version string | Capable | `has_min_mobile_version: false` |
/// |---|--:|--:|
/// | `eero/3.0 (iPhone; iOS 17.0)` (the previous default, [`LEGACY_USER_AGENT`]) | 41 | 44 |
/// | `eero/6.16.0 (iPhone; iOS 17.4.1)` | 66 | 10 |
/// | `eero-ios/2026.8.1 (iPhone; iOS 18.5)` (this default) | 76 | 1 |
/// | `eero-android/2026.8.1` | 77 | 1 |
///
/// The product token matters as well as the version — `eero/2026.8.1` scores 70 where
/// `eero-ios/2026.8.1` scores 76 — so both were updated together. The capabilities the previous
/// default gave up include `led_action`, `device_blacklist`, `device_management`, `diagnostics`,
/// `device_usage`, `unified_content_filters`, `block_apps`, `block_apps_categories`,
/// `ddns_enabled`, `port_forward_range`, `historical_usage`, `ac_compat`, and the five
/// `dnsfilter_*` entries.
///
/// Side effect: under this default the network's `resources` map points `forwards`, `routing` and
/// `support` at API 2.3 instead of 2.2. `forwards` and `routing` return the same shape on both
/// versions; `support` does not — 2.3 returns a `contacts` array where 2.2 returns a flat object.
/// The `get_support`, `get_forwards` and `get_routing` endpoint methods pin their requests to
/// 2.2 regardless of what a parent envelope publishes, so their return shape does not change.
/// Pass [`LEGACY_USER_AGENT`] to keep every link on 2.2.
pub const DEFAULT_USER_AGENT: &str = "eero-ios/2026.8.1 (iPhone; iOS 18.5)";

/// The previous default `User-Agent`, kept available so its (lower-capability) behaviour can
/// still be reproduced — see [`DEFAULT_USER_AGENT`] for the capability comparison.
///
/// Ported from `LEGACY_USER_AGENT` (`const.py:98-100`). Pass it to
/// `ClientBuilder::user_agent(Some(LEGACY_USER_AGENT.to_owned()))` (or the same method on
/// `TransportBuilder`) to send it on every request.
pub const LEGACY_USER_AGENT: &str = "eero/3.0 (iPhone; iOS 17.0)";

/// Default value of the `X-Accept-Language` header sent on every request.
///
/// Ported from `DEFAULT_ACCEPT_LANGUAGE` (`const.py:103`).
pub const DEFAULT_ACCEPT_LANGUAGE: &str = "en-US";

/// Name of the form field the `logout` request's body carries the session-cookie-shaped value
/// under.
///
/// Ported from `LOGOUT_COOKIE_FIELD_NAME` (`const.py:62`). This is a literal field name in a
/// `application/x-www-form-urlencoded` body — **not** the real HTTP `Cookie` header, and not
/// JSON. See [`SESSION_COOKIE_PREFIX`] for the value shape.
pub const LOGOUT_COOKIE_FIELD_NAME: &str = "Cookie";

/// Prefix prepended to the session token to build the value of the `logout` request's
/// `LOGOUT_COOKIE_FIELD_NAME` form field.
///
/// Ported from `SESSION_COOKIE_PREFIX` (`const.py:63`). Distinct from [`SESSION_COOKIE_NAME`]
/// (`"s"`, an HTTP cookie *name*): this is the literal `"s="` text prefix of a form *value*.
pub const SESSION_COOKIE_PREFIX: &str = "s=";

/// Fixed delay between bounded `GET` retries (transport errors / 5xx only).
///
/// Ported from `GET_RETRY_DELAY_SECONDS` (`const.py:112`, `0.5`). Applied by
/// [`crate::transport::Transport`]'s bounded `GET`-only retry loop
/// (`TransportBuilder::get_retries`).
pub const GET_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Version marker written to every persisted credential record.
///
/// Ported from `CREDENTIAL_SCHEMA_VERSION` (`const.py:118`). A record loaded without this key
/// predates the marker — possibly carrying now-unsupported fields such as a legacy `user_token`,
/// `refresh_token`, or `session_expiry` — and is migrated in place on load
/// (`crate::auth::session::Session::from_stored`).
pub const CREDENTIAL_SCHEMA_VERSION: u32 = 2;

/// Wall-clock cap a caller waits for an in-flight session refresh it did not initiate before
/// giving up and surfacing its own original error instead of a fabricated one.
///
/// Ported from `_SESSION_REFRESH_GUARD_TIMEOUT_SECONDS` (`auth.py`, `30.0`). Applied by
/// [`crate::transport::Transport::refresh_session`]'s single-flight coalescing.
pub const SESSION_REFRESH_GUARD_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::{
        API_BASE_22, API_BASE_23, API_HOST, API_VERSION_DEFAULT, API_VERSION_DEVICE_WRITES,
        API_VERSION_MULTISTATICIP, API_VERSION_SECONDARY_WAN, CREDENTIAL_SCHEMA_VERSION,
        DEFAULT_ACCEPT_LANGUAGE, DEFAULT_CACHE_TTL, DEFAULT_USER_AGENT, GET_RETRY_DELAY,
        LEGACY_USER_AGENT, LOGOUT_COOKIE_FIELD_NAME, MAX_RESPONSE_BYTES, READ_TIMEOUT,
        REQUEST_TIMEOUT, SESSION_COOKIE_NAME, SESSION_COOKIE_PREFIX, SESSION_REFRESH_GUARD_TIMEOUT,
        USER_TOKEN_HEADER, USER_TOKEN_KEY, api_endpoint,
    };
    use std::time::Duration;

    #[test]
    fn base_urls_are_versioned_variants_of_the_same_host() {
        assert_eq!(API_BASE_22, "https://api-user.e2ro.com/2.2");
        assert_eq!(API_BASE_23, "https://api-user.e2ro.com/2.3");
        assert_eq!(API_BASE_22, format!("{API_HOST}/{API_VERSION_DEFAULT}"));
        assert_eq!(
            API_BASE_23,
            format!("{API_HOST}/{API_VERSION_DEVICE_WRITES}")
        );
    }

    #[test]
    fn api_endpoint_joins_host_and_version() {
        assert_eq!(api_endpoint("2.2"), API_BASE_22);
        assert_eq!(api_endpoint("2.3"), API_BASE_23);
    }

    #[test]
    fn multistaticip_and_secondary_wan_are_both_2_3() {
        assert_eq!(API_VERSION_MULTISTATICIP, "2.3");
        assert_eq!(API_VERSION_SECONDARY_WAN, "2.3");
        assert_eq!(API_VERSION_MULTISTATICIP, API_VERSION_DEVICE_WRITES);
    }

    #[test]
    fn size_limits_match_const_py() {
        assert_eq!(MAX_RESPONSE_BYTES, 10 * 1024 * 1024);
    }

    #[test]
    fn timeouts_match_python_client_timeout() {
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));
        assert_eq!(READ_TIMEOUT, Duration::from_secs(10));
        assert!(READ_TIMEOUT < REQUEST_TIMEOUT);
    }

    #[test]
    fn cache_ttl_matches_python_default() {
        assert_eq!(DEFAULT_CACHE_TTL, Duration::from_secs(60));
    }

    #[test]
    fn session_cookie_name_is_a_single_letter() {
        assert_eq!(SESSION_COOKIE_NAME, "s");
    }

    #[test]
    fn user_token_header_and_key_match_const_py() {
        assert_eq!(USER_TOKEN_HEADER, "X-User-Token");
        assert_eq!(USER_TOKEN_KEY, "user_token");
    }

    #[test]
    fn default_headers_match_const_py() {
        assert_eq!(DEFAULT_USER_AGENT, "eero-ios/2026.8.1 (iPhone; iOS 18.5)");
        assert_eq!(DEFAULT_ACCEPT_LANGUAGE, "en-US");
    }

    #[test]
    fn legacy_user_agent_is_the_previous_default() {
        assert_eq!(LEGACY_USER_AGENT, "eero/3.0 (iPhone; iOS 17.0)");
        assert_ne!(LEGACY_USER_AGENT, DEFAULT_USER_AGENT);
    }

    #[test]
    fn both_user_agents_are_printable_ascii_without_cr_lf() {
        // Mirrors `_validate_header_value` (`api/base.py:120-133`): the defaults must pass the
        // very check that guards a caller-supplied value.
        for value in [DEFAULT_USER_AGENT, LEGACY_USER_AGENT] {
            assert!(
                value.bytes().all(|b| (0x20..=0x7E).contains(&b)),
                "{value:?}"
            );
        }
    }

    #[test]
    fn logout_cookie_field_shape_matches_const_py() {
        assert_eq!(LOGOUT_COOKIE_FIELD_NAME, "Cookie");
        assert_eq!(SESSION_COOKIE_PREFIX, "s=");
    }

    #[test]
    fn get_retry_delay_matches_const_py() {
        assert_eq!(GET_RETRY_DELAY, Duration::from_millis(500));
    }

    #[test]
    fn credential_schema_version_matches_const_py() {
        assert_eq!(CREDENTIAL_SCHEMA_VERSION, 2);
    }

    #[test]
    fn session_refresh_guard_timeout_matches_auth_py() {
        assert_eq!(SESSION_REFRESH_GUARD_TIMEOUT, Duration::from_secs(30));
    }
}
