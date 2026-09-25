//! Auth routes (`AuthApi`): login, verify, resend, logout, session refresh.

// ------------------------------ auth (`AuthApi`) ------------------------------
//
// The GET /account route lives in `super::networks` (`ACCOUNT`) alongside the rest of
// the account/network-shaped resources.

use super::{ApiVersion, Route};
use reqwest::Method;

/// `POST /2.2/login` — start the email/SMS login handshake.
///
/// Ported from `const.py:14` (`LOGIN_ENDPOINT`); see `api/auth.py:87-143`.
pub const LOGIN: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login",
};

/// `POST /2.2/login/verify` — complete the login handshake with the emailed/texted code.
///
/// Ported from `const.py:15` (`LOGIN_VERIFY_ENDPOINT`); see `api/auth.py:145-201`.
pub const LOGIN_VERIFY: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/verify",
};

/// `POST /2.2/login/resend` — ask the server to resend the verification code.
///
/// Ported from `const.py:14` + `auth.py:114,224` (`f"{LOGIN_ENDPOINT}/resend"`); given its own
/// `Route` here rather than being built by string concatenation at call time.
pub const LOGIN_RESEND: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/resend",
};

/// `POST /2.2/logout` — end the current session.
///
/// Ported from `const.py:16` (`LOGOUT_ENDPOINT`); see `api/auth.py:239-277`.
pub const LOGOUT: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "logout",
};

/// `POST /2.2/login/refresh` — first of the two session-refresh routes the client tries, in
/// order.
///
/// Ported from `const.py:22` (`LOGIN_REFRESH_ENDPOINT`); see `api/auth.py:299` (`REFRESH_ENDPOINTS`
/// iteration) and the port plan §7.2 (neither refresh route is confirmed to return a token; a
/// normal login never yields a refresh token, so this path is expected to be a practical no-op).
pub const LOGIN_REFRESH: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/refresh",
};

/// `POST /2.2/account/refresh` — second of the two session-refresh routes the client tries.
///
/// Ported from `const.py:23` (`ACCOUNT_REFRESH_ENDPOINT`); see [`LOGIN_REFRESH`] for the retry
/// order and caveats.
pub const ACCOUNT_REFRESH: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "account/refresh",
};
