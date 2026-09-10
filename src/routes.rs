//! Every wire endpoint as a [`Route`] constant.
//!
//! Endpoint modules never build URLs by hand; upstream drift is a one-line fix here.
//!
//! There is no Python original for this module: `eero-api` builds each URL inline, string by
//! string, in every `api/*.py` file (e.g. `f"{API_ENDPOINT}/networks/{network_id}/devices"`).
//! `rusteero` centralises the same information as data — a [`Route`] pairs an HTTP
//! [`Method`], an [`ApiVersion`] (which selects the base host), and a path template — so that a
//! server-side path rename or version bump is a single-line edit with one wiremock test to
//! match, instead of a grep-and-replace across two dozen endpoint modules (port plan §7.1).
//!
//! ## Path templates and rendering
//!
//! A path template is a `'static` string of `/`-separated segments. A segment written as
//! `{name}` is a placeholder that [`Route::render`] fills in from the caller-supplied
//! `params` at request time; every other segment is a literal, written by us, and passed
//! through unchanged (aside from the same percent-encoding pass every segment gets — see
//! below).
//!
//! [`Route::render`] is total: for any `path` and any `params`, it returns either `Ok(Url)` or
//! `Err(RenderError)`, and never panics. Two design choices worth calling out:
//!
//! - **Missing placeholder → `Err`, not left unsubstituted.** If a template segment is
//!   `{name}` and `params` has no entry for `name`, rendering fails with
//!   `RenderError::MissingPlaceholder`. Silently emitting the literal text `{name}` into a
//!   request URL would send a malformed, likely-404 request to the real API with no compile-time
//!   or type-level signal that a caller forgot a parameter; failing fast is safer and matches
//!   this crate's "every fallible operation returns `Result`" convention.
//! - **Percent-encoding uses only the `url` crate's own segment-builder**, [`Url::path_segments_mut`],
//!   never hand-rolled string concatenation. Each segment (literal or substituted) is pushed
//!   through [`url::PathSegmentsMut::push`], which percent-encodes it for the path-segment
//!   position — including `%`, `/`, and `?`, none of which can therefore ever terminate the
//!   segment early or introduce a new path segment, query string, or fragment. This is what
//!   makes device MACs, network ids, or any other user/API-controlled string safe to interpolate
//!   directly.
//!
//! One quirk inherited from the `url` crate (and, transitively, the WHATWG URL spec) is worth
//! documenting rather than working around: a segment value that is exactly `"."` or `".."` is
//! *dropped* by [`Url::path_segments_mut`] rather than appended (dot-segment removal). A `{id}`
//! placeholder filled with `".."` therefore does not error and does not escape into the parent
//! path — it simply vanishes, shortening the rendered path by one segment. Real Eero identifiers
//! (integers, UUIDs, MAC addresses) never take this value, so this is a documented edge case, not
//! a mitigation for a realistic input.

use crate::consts;
use reqwest::Method;
use url::Url;

/// Selects which Eero cloud API host/version a [`Route`] targets.
///
/// Ported from the fact — not the code shape — of `const.py:7,13`: `eero-api` hardcodes two
/// base URLs (`API_ENDPOINT` for almost everything, `DEVICE_UPDATE_ENDPOINT` for device
/// nickname/pause writes only). `rusteero` keeps the same two hosts but names them as an enum so
/// every [`Route`] states its version explicitly instead of picking a base-URL constant by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiVersion {
    /// API version 2.2 — the default for almost every endpoint.
    ///
    /// Base URL: [`consts::API_BASE_22`].
    V2_2,
    /// API version 2.3 — required for device-mutation writes (nickname, pause); see
    /// [`consts::API_BASE_23`] for why 2.2 cannot be used for those instead.
    V2_3,
}

impl ApiVersion {
    /// Returns the base URL for this API version.
    #[must_use]
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::V2_2 => consts::API_BASE_22,
            Self::V2_3 => consts::API_BASE_23,
        }
    }
}

/// A single wire endpoint: an HTTP method, an API version (which selects the base host), and a
/// `'static` path template.
///
/// `Route` values are plain data — constructing one never fails and never touches the network.
/// Every field is `pub` so a `Route` can be built as a `const` (see the constants below) or
/// assembled ad hoc (e.g. in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// HTTP verb this endpoint expects.
    pub method: Method,
    /// Which API version (and therefore base host) this endpoint is served from.
    pub version: ApiVersion,
    /// `/`-separated path template, relative to [`ApiVersion::base_url`]. Segments written as
    /// `{name}` are placeholders filled in by [`Route::render`]; every other segment is a
    /// literal.
    pub path: &'static str,
}

/// Error produced by [`Route::render`] when a path template cannot be turned into a request
/// [`Url`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// A `{name}` placeholder in the route's path template had no corresponding entry in the
    /// `params` passed to [`Route::render`].
    ///
    /// Carries the placeholder's name, without the surrounding braces.
    MissingPlaceholder(String),
    /// The route's base URL ([`ApiVersion::base_url`]) failed to parse as a [`Url`].
    ///
    /// Unreachable for every [`Route`] defined in this module — both base URLs are fixed,
    /// well-formed `https://` literals — but kept as a real error variant (rather than a panic
    /// or `unwrap`) so [`Route::render`] stays total even if a future base URL is ever
    /// misconfigured.
    InvalidBaseUrl,
    /// The route's base URL cannot be used as a base for additional path segments (for example,
    /// a `cannot-be-a-base` URL such as `mailto:` or `data:`).
    ///
    /// Unreachable for every [`Route`] defined in this module, for the same reason as
    /// [`RenderError::InvalidBaseUrl`].
    CannotExtendBase,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPlaceholder(name) => {
                write!(
                    f,
                    "route path is missing a value for placeholder `{{{name}}}`"
                )
            }
            Self::InvalidBaseUrl => write!(f, "route base URL failed to parse"),
            Self::CannotExtendBase => write!(f, "route base URL cannot be used as a path base"),
        }
    }
}

impl std::error::Error for RenderError {}

impl Route {
    /// Renders this route's path template against `params`, substituting each `{name}`
    /// placeholder with the value from the matching `(name, value)` pair, and returns the full
    /// request [`Url`] (base host + rendered path).
    ///
    /// `params` is searched linearly; with the handful of placeholders any real route template
    /// has, this is both simpler and faster than building a map. Every path segment — literal or
    /// substituted — is percent-encoded for the path-segment position via
    /// [`Url::path_segments_mut`]; see the module docs for exactly what that guarantees and the
    /// one documented edge case (`"."` / `".."` segments are dropped, not escaped).
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::MissingPlaceholder`] if the template references a name absent from
    /// `params`. Returns [`RenderError::InvalidBaseUrl`] or [`RenderError::CannotExtendBase`] only
    /// if [`ApiVersion::base_url`] itself is malformed, which cannot happen for any `Route`
    /// defined in this module.
    ///
    /// This function never panics for any `path` or `params` value.
    pub fn render(&self, params: &[(&str, &str)]) -> Result<Url, RenderError> {
        let mut url =
            Url::parse(self.version.base_url()).map_err(|_| RenderError::InvalidBaseUrl)?;

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|()| RenderError::CannotExtendBase)?;

            for part in self.path.split('/') {
                if part.is_empty() {
                    // Leading/trailing/doubled slashes in a template contribute no segment.
                    continue;
                }

                if let Some(name) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let value = params
                        .iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| *value)
                        .ok_or_else(|| RenderError::MissingPlaceholder(name.to_owned()))?;
                    segments.push(value);
                } else {
                    segments.push(part);
                }
            }
        }

        Ok(url)
    }
}

// =====================================================================================
// Auth + account routes (phase 1). Every other domain (networks, devices, eeros,
// profiles, ...) is added in phase 3 by a dedicated task — do not add routes below this
// banner until that task lands; two agents must never edit this file concurrently.
// =====================================================================================

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

/// `GET /2.2/account` — fetch the account resource.
///
/// Ported from `const.py:17` (`ACCOUNT_ENDPOINT`) — a constant the Python source itself never
/// imports outside `const.py` (dead in `eero-api`), but the endpoint is real and used by
/// `rusteero`'s `Client::get_account` and the `get_networks` `/account` fallback (port plan
/// §1.6).
pub const ACCOUNT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "account",
};

#[cfg(test)]
mod tests {
    use super::{ApiVersion, RenderError, Route};
    use reqwest::Method;

    // ----------------------------- ApiVersion::base_url -----------------------------

    #[test]
    fn v2_2_base_url_is_the_2_2_host() {
        assert_eq!(ApiVersion::V2_2.base_url(), "https://api-user.e2ro.com/2.2");
    }

    #[test]
    fn v2_3_base_url_is_the_2_3_host() {
        assert_eq!(ApiVersion::V2_3.base_url(), "https://api-user.e2ro.com/2.3");
    }

    // ------------------------------- required routes ---------------------------------

    #[test]
    fn required_auth_and_account_routes_have_the_expected_verb_version_and_path() {
        use super::{
            ACCOUNT, ACCOUNT_REFRESH, LOGIN, LOGIN_REFRESH, LOGIN_RESEND, LOGIN_VERIFY, LOGOUT,
        };

        assert_eq!(LOGIN.method, Method::POST);
        assert_eq!(LOGIN.version, ApiVersion::V2_2);
        assert_eq!(LOGIN.path, "login");

        assert_eq!(LOGIN_VERIFY.method, Method::POST);
        assert_eq!(LOGIN_VERIFY.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_VERIFY.path, "login/verify");

        assert_eq!(LOGIN_RESEND.method, Method::POST);
        assert_eq!(LOGIN_RESEND.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_RESEND.path, "login/resend");

        assert_eq!(LOGOUT.method, Method::POST);
        assert_eq!(LOGOUT.version, ApiVersion::V2_2);
        assert_eq!(LOGOUT.path, "logout");

        assert_eq!(LOGIN_REFRESH.method, Method::POST);
        assert_eq!(LOGIN_REFRESH.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_REFRESH.path, "login/refresh");

        assert_eq!(ACCOUNT_REFRESH.method, Method::POST);
        assert_eq!(ACCOUNT_REFRESH.version, ApiVersion::V2_2);
        assert_eq!(ACCOUNT_REFRESH.path, "account/refresh");

        assert_eq!(ACCOUNT.method, Method::GET);
        assert_eq!(ACCOUNT.version, ApiVersion::V2_2);
        assert_eq!(ACCOUNT.path, "account");
    }

    #[test]
    fn login_and_verify_render_to_the_expected_absolute_urls() {
        use super::{LOGIN, LOGIN_VERIFY};

        assert_eq!(
            LOGIN.render(&[]).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login"
        );
        assert_eq!(
            LOGIN_VERIFY.render(&[]).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login/verify"
        );
    }

    // ------------------------------ placeholder rendering ------------------------------

    #[test]
    fn placeholder_is_substituted_with_the_supplied_value() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let url = route
            .render(&[("network_id", "123")])
            .expect("all placeholders supplied");
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/123/devices"
        );
    }

    #[test]
    fn multiple_placeholders_are_each_substituted_independently() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_3,
            path: "networks/{network_id}/devices/{device_id}",
        };

        let url = route
            .render(&[("network_id", "123"), ("device_id", "ab:cd")])
            .expect("all placeholders supplied");
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.3/networks/123/devices/ab:cd"
        );
    }

    #[test]
    fn missing_placeholder_value_is_a_render_error_not_a_panic_or_silent_gap() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let err = route.render(&[]).unwrap_err();
        assert_eq!(
            err,
            RenderError::MissingPlaceholder("network_id".to_owned())
        );
    }

    #[test]
    fn extra_unused_params_are_ignored() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "account",
        };

        let url = route
            .render(&[("network_id", "123"), ("unused", "x")])
            .expect("no placeholders required, extra params are harmless");
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/account");
    }

    // ------------------------------- percent-encoding -----------------------------------

    #[test]
    fn slash_and_question_mark_in_a_placeholder_value_cannot_escape_their_segment() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let url = route
            .render(&[("network_id", "abc/def?x=1")])
            .expect("value is escaped, not rejected");

        // The hostile value is confined to a single, opaque path segment: no extra "/devices"
        // path segment was introduced, and no query string was introduced either.
        assert_eq!(url.path(), "/2.2/networks/abc%2Fdef%3Fx=1/devices");
        assert_eq!(url.query(), None);
        let segments: Vec<&str> = url.path_segments().unwrap().collect();
        assert_eq!(segments, ["2.2", "networks", "abc%2Fdef%3Fx=1", "devices"]);
    }

    #[test]
    fn percent_sign_in_a_placeholder_value_is_itself_escaped() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}",
        };

        let url = route.render(&[("network_id", "100%")]).unwrap();
        assert_eq!(url.path(), "/2.2/networks/100%25");
    }
}
