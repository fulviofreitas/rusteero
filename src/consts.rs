//! Wire constants: base URLs, size limits, timeouts, and other values ported from `eero-api`'s
//! `src/eero/const.py`.
//!
//! ## Deliberate omissions
//!
//! - **`DEFAULT_HEADERS`** (`const.py:29-32`) is not ported. The Python library copies this
//!   dict onto `self._headers`, but the `session.request(...)` call site that actually performs
//!   the HTTP request never passes `headers=` (`api/base.py:79,167`), so the mobile-style
//!   `User-Agent` string is set but never sent on the wire. `rusteero` replicates the *observed*
//!   wire behaviour rather than the unused Python constant and sends reqwest's default
//!   `User-Agent` (decision D-7); a caller may still override it explicitly on the client
//!   builder.
//! - **`EeroDeviceType`, `EeroNetworkStatus`, `EeroDeviceStatus`** (`const.py:49-75`) are not
//!   ported. All three are dead code in the Python library — nothing in `src/eero/` imports or
//!   branches on them (confirmed by grep across the package) — so there is no observable
//!   serialization behaviour to preserve. Dropped per the port plan, §3.1.

use std::time::Duration;

/// Base URL for API version 2.2 — the default version for almost every endpoint.
///
/// Ported from `const.py:7` (`API_ENDPOINT`).
pub const API_BASE_22: &str = "https://api-user.e2ro.com/2.2";

/// Base URL for API version 2.3.
///
/// Device-mutation writes (nickname, pause) are silently dropped by version 2.2: the server
/// accepts the `PUT` and returns `200 OK`, but the change never persists server-side. Version
/// 2.3 processes the same writes correctly; reads and every other resource stay on 2.2. Ported
/// from `const.py:9-13` (`DEVICE_UPDATE_ENDPOINT`); see `eero-api` issue #102.
pub const API_BASE_23: &str = "https://api-user.e2ro.com/2.3";

/// Maximum number of bytes read from a single response body before the request is aborted.
///
/// Guards against unbounded memory consumption from a hostile or misbehaving upstream response.
/// Ported from `const.py:38`.
pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum number of characters of a response body embedded in an error message.
///
/// Caps log/error amplification when an upstream returns an oversized or hostile body. Ported
/// from `const.py:42`.
pub const MAX_ERROR_BODY_CHARS: usize = 512;

/// Overall request timeout, from connect through to the full response body.
///
/// Ported from `api/base.py:145-146`'s `aiohttp.ClientTimeout(total=30, sock_read=10)`; this is
/// the `total` half of that pair. See [`READ_TIMEOUT`] for the `sock_read` half.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-read timeout applied while streaming a response body.
///
/// Ported from `api/base.py:145-146`'s `sock_read=10`.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Default time-to-live for cached responses.
///
/// Ported from `const.py:35` (`CACHE_TIMEOUT`). Note that in the Python source this constant is
/// itself unused: `client.py:41,52` hardcodes the literal `60` again for `EeroClient`'s
/// `cache_timeout` default rather than importing `CACHE_TIMEOUT`. The two values coincide today
/// but are not the same symbol upstream; `rusteero`'s `Client` should read its default from this
/// constant instead.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60);

/// Number of days a client-fabricated session is considered valid.
///
/// The Eero cloud API never returns a session expiry itself; `eero-api` fabricates one at
/// `now + 30 days` after `verify()`, `refresh_session()`, and `set_session_token()`
/// (`api/auth.py:180-182,323-325,411-413`). `rusteero` replicates this client-side expiry
/// convention rather than trusting a server-provided value.
pub const SESSION_LIFETIME_DAYS: i64 = 30;

/// Name of the cookie that carries the session token on every authenticated request.
///
/// Ported from `api/base.py:149-152`, where the session id is set as cookie `s=<token>` on the
/// HTTP client before each authenticated request; there is no `Authorization` header anywhere
/// in the Eero cloud API.
pub const SESSION_COOKIE_NAME: &str = "s";

/// Value of `meta.error` that signals the server wants the client to refresh its session and
/// retry the request exactly once.
///
/// Ported from `api/base.py:226,238` (`"error.session.refresh"`). This literal is not a
/// module-level constant in `const.py` itself — the Python source inlines the string directly in
/// `api/base.py` — but it is promoted to a named constant here since `transport.rs` and
/// `auth/*` both need to agree on the exact value.
pub const REFRESH_ERROR_CODE: &str = "error.session.refresh";

/// Wire key holding the new session token in a `login/refresh` or `account/refresh` response
/// body.
///
/// Ported from `const.py:45` (`SESSION_TOKEN_KEY`); used only when parsing the response of
/// `refresh_session()` (`api/auth.py:319`). This is a *different* wire key from the initial
/// login/verify handshake, which reads `user_token` directly instead.
pub const SESSION_TOKEN_KEY: &str = "session_token";

/// Wire key for a refresh token in a refresh-response payload.
///
/// Ported from `const.py:46` (`REFRESH_TOKEN_KEY`) for a literal, constant-for-constant parity
/// with `const.py`. The Python library never actually imports this constant: every call site
/// (`api/auth.py:320`, `api/auth_storage.py:37,54,78`) uses the literal string `"refresh_token"`
/// instead (confirmed by grep of `src/eero/`), so it is dead code upstream. Kept here so the
/// same dead-code fact is visible in the port rather than silently dropped.
pub const REFRESH_TOKEN_KEY: &str = "refresh_token";

#[cfg(test)]
mod tests {
    use super::{
        API_BASE_22, API_BASE_23, DEFAULT_CACHE_TTL, MAX_ERROR_BODY_CHARS, MAX_RESPONSE_BYTES,
        READ_TIMEOUT, REQUEST_TIMEOUT, SESSION_COOKIE_NAME, SESSION_LIFETIME_DAYS,
    };
    use std::time::Duration;

    #[test]
    fn base_urls_are_versioned_variants_of_the_same_host() {
        assert_eq!(API_BASE_22, "https://api-user.e2ro.com/2.2");
        assert_eq!(API_BASE_23, "https://api-user.e2ro.com/2.3");
    }

    #[test]
    fn size_limits_match_const_py() {
        assert_eq!(MAX_RESPONSE_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_ERROR_BODY_CHARS, 512);
    }

    #[test]
    fn timeouts_match_python_client_timeout() {
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));
        assert_eq!(READ_TIMEOUT, Duration::from_secs(10));
        assert!(READ_TIMEOUT < REQUEST_TIMEOUT);
    }

    #[test]
    fn cache_ttl_and_session_lifetime_match_python_defaults() {
        assert_eq!(DEFAULT_CACHE_TTL, Duration::from_secs(60));
        assert_eq!(SESSION_LIFETIME_DAYS, 30);
    }

    #[test]
    fn session_cookie_name_is_a_single_letter() {
        assert_eq!(SESSION_COOKIE_NAME, "s");
    }
}
