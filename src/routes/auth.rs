//! Auth routes (`AuthApi`): login, verify, resend, logout, session refresh.

// ------------------------------ auth (`AuthApi`) ------------------------------
//
// The GET /account route lives in `super::networks` (`ACCOUNT`) alongside the rest of
// the account/network-shaped resources.
//
// Every route below is a fixed path (no `{id}` placeholder), so it is modelled as a
// [`Resource`] with `link: None` — `Resource::resolve` ignores its `id_or_url` argument
// entirely for a template with no placeholder (see `Resource::resolve`'s own docs) — and
// resolved with an empty id and no parent at every call site.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `POST /2.2/login` — start the email/SMS login handshake.
///
/// Ported from `const.py:14` (`LOGIN_ENDPOINT`); see `api/auth.py:87-143`.
pub const LOGIN: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "login",
    link: None,
};

/// `POST /2.2/login/verify` — complete the login handshake with the emailed/texted code.
///
/// Ported from `const.py:15` (`LOGIN_VERIFY_ENDPOINT`); see `api/auth.py:145-201`.
pub const LOGIN_VERIFY: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "login/verify",
    link: None,
};

/// `POST /2.2/login/resend` — ask the server to resend the verification code.
///
/// Ported from `const.py:14` + `auth.py:114,224` (`f"{LOGIN_ENDPOINT}/resend"`); given its own
/// route here rather than being built by string concatenation at call time.
pub const LOGIN_RESEND: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "login/resend",
    link: None,
};

/// `POST /2.2/logout` — end the current session.
///
/// Ported from `const.py:16` (`LOGOUT_ENDPOINT`); see `api/auth.py:239-277`.
pub const LOGOUT: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "logout",
    link: None,
};

/// `POST /2.2/login/refresh` — the single session-refresh route at `v8.0.4`.
///
/// Ported from `const.py:56` (`LOGIN_REFRESH_ENDPOINT`); see `api/auth.py:415-477`. `v6.2.0`'s
/// `account/refresh` fallback route (`ACCOUNT_REFRESH_ENDPOINT`) is gone at `v8.0.4` — there is
/// exactly one refresh route now, no try-next-route loop. Authenticated by the *current* session
/// token itself (there is no separate refresh token at `v8.0.4`); the server-issued token in the
/// response is discarded per SDK policy — see [`crate::transport::Transport::refresh_session`].
pub const LOGIN_REFRESH: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "login/refresh",
    link: None,
};
