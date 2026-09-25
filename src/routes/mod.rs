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
//! `Err(RenderError)`, and never panics. Design choices worth calling out:
//!
//! - **Missing placeholder → `Err`, not left unsubstituted.** If a template segment is
//!   `{name}` and `params` has no entry for `name`, rendering fails with
//!   `RenderError::MissingPlaceholder`. Silently emitting the literal text `{name}` into a
//!   request URL would send a malformed, likely-404 request to the real API with no compile-time
//!   or type-level signal that a caller forgot a parameter; failing fast is safer and matches
//!   this crate's "every fallible operation returns `Result`" convention.
//! - **Every substituted value is validated before it reaches the encoder**, by
//!   `validate_segment` — see the "Path traversal" section below for why this check exists
//!   and exactly what it rejects.
//! - **Percent-encoding uses only the `url` crate's own segment-builder**, [`Url::path_segments_mut`],
//!   never hand-rolled string concatenation. Each segment (literal or substituted) that passes
//!   validation is pushed through [`url::PathSegmentsMut::push`], which percent-encodes it for
//!   the path-segment position — including `%`, `/`, and `?`, none of which can therefore ever
//!   terminate the segment early or introduce a new path segment, query string, or fragment.
//!
//! ## Path traversal (formerly documented, incorrectly, as impossible)
//!
//! An earlier revision of this module claimed that a `".."` placeholder value "does not error
//! and does not escape into the parent path" because [`Url::path_segments_mut`] drops a segment
//! that is *exactly* `"."` or `".."` rather than appending it. That claim was false for any
//! value that is not already, byte-for-byte, `"."` or `".."`.
//!
//! `url` 2.5.8's path parser strips every ASCII tab, carriage return, and line feed from a
//! segment **before** applying dot-segment removal. A value such as `"..\n"` is therefore not
//! `".."` when `validate_segment` (or, pre-fix, nothing at all) sees it, but *is* `".."` by the
//! time the encoder's dot-segment check runs — so it collapses the rendered path exactly as a
//! literal `".."` would, silently walking a destructive verb (a `DELETE` or a `/2.3` `PUT`) onto
//! the parent collection instead of the one item the caller named. A stray trailing newline on
//! an id read from a file, an environment variable, or `$(cat …)` is enough to trigger this —
//! no attacker input is required. A second, related gap: an **empty** substituted value collapses
//! a per-item route onto its collection while keeping the same verb (`DELETE
//! .../blacklist/{id}` with `id = ""` renders `DELETE .../blacklist/`), which — if the server
//! treats that path as "the collection" — turns a "remove one" into "remove all".
//!
//! What is actually guaranteed today: `validate_segment` runs on every substituted value
//! before it is pushed onto the URL, and rejects it outright — the request is never sent — if
//! the value is empty, contains any ASCII control character (`0x00`-`0x1F` or `0x7F`, which
//! already covers every byte the encoder's tab/CR/LF-stripping pass would otherwise remove), or
//! is exactly `"."` or `".."` after that stripping. [`Route::render`] and `Transport`'s own
//! renderer (`transport.rs`) share this exact rule by calling the same function, so the
//! guarantee holds identically for both. A value containing an *already-encoded* dot segment
//! (e.g. `"%2e%2e"`), a slash (`"a/b"`), or non-ASCII look-alike dots (e.g. fullwidth `"．．"`)
//! is unaffected by this check and continues to be percent-encoded into a single opaque segment
//! as before — none of those can reach the parser's dot-segment logic at all.

use crate::consts;
use crate::error::Error;
use reqwest::Method;
use serde_json::Value;
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

    /// Returns the bare version path segment for this API version (`"2.2"` or `"2.3"`), the
    /// single source of truth [`Self::base_url`] and every `links`/`params` caller derive their
    /// version string from, so a future version bump only ever needs one edit.
    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::V2_2 => consts::API_VERSION_DEFAULT,
            Self::V2_3 => consts::API_VERSION_DEVICE_WRITES,
        }
    }
}

/// A resource addressed by one id-or-url plus an optional published link name — the
/// `resource_url`/`sub_resource_url` shape from [`crate::links`], attached to a `'static` route
/// definition so a domain module never builds the URL by hand.
///
/// `template` must contain exactly one `{id}` placeholder (e.g. `"networks/{id}/settings"`), or
/// none at all for a fixed path that ignores its `id_or_url` argument entirely (e.g.
/// `"account"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// HTTP verb this endpoint expects.
    pub method: Method,
    /// Which API version (and therefore base host) this endpoint is served from.
    pub version: ApiVersion,
    /// A [`crate::links::resource_url`]-shaped template: exactly one `{id}` placeholder, or none
    /// for a fixed path.
    pub template: &'static str,
    /// The name of the link in the parent's `resources` object that, when the caller supplies a
    /// parent envelope and the link is present, is preferred over `template` entirely. `None`
    /// when this resource is never published as a named link.
    pub link: Option<&'static str>,
}

impl Resource {
    /// Resolves this resource's absolute URL against `host`.
    ///
    /// When [`Self::template`] contains no `{id}` placeholder, resolves to `{host}/{version}/
    /// {template}` and `id_or_url` is ignored. Otherwise: link-preferring via
    /// [`crate::links::sub_resource_url`] when [`Self::link`] is `Some`, or
    /// [`crate::links::resource_url`] otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] as the underlying `links` helper.
    pub fn resolve(
        &self,
        host: &Url,
        id_or_url: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        if !self.template.contains("{id}") {
            let base = host.as_str().trim_end_matches('/');
            let segment = self.version.segment();
            let path = self.template.trim_matches('/');
            let full = format!("{base}/{segment}/{path}");
            return Url::parse(&full)
                .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")));
        }
        match self.link {
            Some(link) => crate::links::sub_resource_url(
                host,
                id_or_url,
                self.template,
                link,
                parent,
                self.version,
            ),
            None => crate::links::resource_url(host, id_or_url, self.template, self.version),
        }
    }
}

/// A two-level nested resource — `networks/{network}/{prefix}/{child}{suffix}` — the
/// `resolve_nested_url` shape from [`crate::params`], attached to a `'static` route definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nested {
    /// HTTP verb this endpoint expects.
    pub method: Method,
    /// Which API version (and therefore base host) this endpoint is served from.
    pub version: ApiVersion,
    /// The literal path segment(s) between the network and the child id, e.g. `"profiles"` or
    /// `"insights/devices"`. No leading or trailing slash.
    pub prefix: &'static str,
    /// A literal path segment appended after the child id, e.g. `"/schedules"`. Empty when the
    /// child id is the final path segment.
    pub suffix: &'static str,
    /// The name of the link in the parent's `resources` object that, when present, is preferred
    /// over the network/child template entirely.
    pub link: Option<&'static str>,
}

impl Nested {
    /// Resolves this nested resource's absolute URL against `host`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] as [`crate::params::resolve_nested_url`].
    pub fn resolve(
        &self,
        host: &Url,
        network: &str,
        child: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        crate::params::resolve_nested_url(
            host,
            network,
            child,
            self.prefix,
            self.suffix,
            self.link,
            parent,
            self.version,
        )
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

/// Why `validate_segment` rejected a value before it could be substituted into a rendered
/// path segment.
///
/// This is the single validation rule shared by [`Route::render`] and `Transport`'s own
/// renderer (`transport.rs`'s `render_url`) — see the module docs' "Path traversal" section for
/// the vulnerability this closes and exactly what each variant means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentError {
    /// The value is empty.
    ///
    /// An empty substituted value would otherwise render as *no segment at all*, collapsing a
    /// per-item route onto its parent collection while keeping the same (potentially
    /// destructive) HTTP verb.
    Empty,
    /// The value contains an ASCII control character (`0x00`-`0x1F` or `0x7F`).
    ///
    /// This already covers every ASCII tab, carriage return, and line feed — the exact bytes
    /// `url` 2.5.8 strips from a path segment before applying dot-segment removal, which is what
    /// makes a value such as `"..\n"` behave as `".."` once it reaches the encoder.
    ControlCharacter,
    /// The value, with every ASCII tab, carriage return, and line feed removed, is exactly `"."`
    /// or `".."`.
    ///
    /// A value that already contains one of those bytes is rejected earlier, as
    /// [`SegmentError::ControlCharacter`]; this variant catches the remaining case — a literal
    /// `"."` or `".."` with no whitespace at all — which the `url` crate would otherwise drop
    /// silently instead of treating as an error.
    DotSegment,
}

impl std::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "must not be empty"),
            Self::ControlCharacter => {
                write!(f, "must not contain an ASCII control character")
            }
            Self::DotSegment => write!(f, "must not be a \".\" or \"..\" path segment"),
        }
    }
}

impl std::error::Error for SegmentError {}

/// Validates that `value` is safe to substitute into a single rendered path segment.
///
/// The one place a caller-supplied value is checked before either renderer in this crate
/// ([`Route::render`] and `Transport`'s own `render_url`) hands it to
/// [`Url::path_segments_mut`]. Validating the *raw* value here, rather than trusting the `url`
/// crate's own encoder to make any value safe, is deliberate: the encoder strips ASCII
/// tab/CR/LF bytes from a segment *before* percent-encoding and dot-segment removal run, so a
/// value that is not literally `".."` can still be treated as `".."` by the time it is written
/// into the URL. See the module docs' "Path traversal" section for the full mechanism and a
/// concrete example.
///
/// # Errors
///
/// Returns [`SegmentError::Empty`] for an empty value, [`SegmentError::ControlCharacter`] for a
/// value containing any ASCII control character (`0x00`-`0x1F` or `0x7F`), or
/// [`SegmentError::DotSegment`] if the value is exactly `"."` or `".."` once every ASCII tab,
/// carriage return, and line feed has been removed from it. A value that does not trip any of
/// these checks is otherwise unrestricted — including one containing `/`, `%`, `?`, `#`, or a
/// non-ASCII character — and is percent-encoded into a single opaque segment exactly as before.
pub(crate) fn validate_segment(value: &str) -> Result<(), SegmentError> {
    if value.is_empty() {
        return Err(SegmentError::Empty);
    }
    if value.bytes().any(|b| matches!(b, 0x00..=0x1F | 0x7F)) {
        return Err(SegmentError::ControlCharacter);
    }
    // Reached only when `value` contains no ASCII tab/CR/LF at all (the loop above already
    // rejected any value that does, since those bytes fall inside `0x00..=0x1F`), so this
    // strip is a no-op in practice — kept explicit so the rule matches, byte for byte, the
    // "tab/CR/LF-stripped form" wording this check is documented (and audited) against.
    let stripped: String = value
        .chars()
        .filter(|&c| c != '\t' && c != '\r' && c != '\n')
        .collect();
    if stripped == "." || stripped == ".." {
        return Err(SegmentError::DotSegment);
    }
    Ok(())
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
    /// A `{name}` placeholder's substituted value was rejected by `validate_segment` before it
    /// could be written into the rendered path.
    InvalidSegment {
        /// The placeholder's name, without the surrounding braces.
        name: String,
        /// Why the value was rejected.
        reason: SegmentError,
    },
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
            Self::InvalidSegment { name, reason } => {
                write!(f, "value for placeholder `{{{name}}}` is invalid: {reason}")
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
    /// has, this is both simpler and faster than building a map. Every substituted value is
    /// first checked by `validate_segment` — see the module docs' "Path traversal" section for
    /// exactly what that guarantees. A value that passes validation is percent-encoded for the
    /// path-segment position via [`Url::path_segments_mut`], same as every literal segment.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::MissingPlaceholder`] if the template references a name absent from
    /// `params`. Returns [`RenderError::InvalidSegment`] if a substituted value fails
    /// `validate_segment`. Returns [`RenderError::InvalidBaseUrl`] or
    /// [`RenderError::CannotExtendBase`] only if [`ApiVersion::base_url`] itself is malformed,
    /// which cannot happen for any `Route` defined in this module.
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
                    validate_segment(value).map_err(|reason| RenderError::InvalidSegment {
                        name: name.to_owned(),
                        reason,
                    })?;
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
// Domain routes (phase 3). Every remaining wire endpoint from the Python `eero-api`
// package gets one `Route` constant per unique (verb, version, path) combination.
// Banners below follow the module order of the port plan's endpoint catalogue (§1.7).
// Several Python methods across different modules turn out to hit the exact same wire
// endpoint (same verb, version and path) — those are modelled as an alias `Route`
// constant (`pub const ALIAS: Route = CANONICAL;`) rather than a second literal, so a
// server-side path change is still a one-line fix.
//
// Deliberately NOT ported here (see the port plan §1.7 and this crate's lessons
// learned in `CLAUDE.md`):
// - `ActivityAPI` (`activity.py`): all five methods (`get_activity`,
//   `get_activity_clients`, `get_activity_for_device`, `get_activity_history`,
//   `get_activity_categories`) target `networks/{id}/activity*` paths that return 404 on
//   both API versions — confirmed dead against a live account (`eero-api` issue #107).
// - `DevicesAPI.set_device_priority` (`devices.py:219`): PUTs the same 2.3 device URL as
//   `SET_DEVICE_NICKNAME`/`PAUSE_DEVICE`, but the server silently ignores the
//   `prioritized`/`priority_duration` fields — a confirmed no-op (`eero-api` issue #111).
//   Giving it its own route would misleadingly suggest a working endpoint.
// =====================================================================================

// One file per Python domain module (port plan §1.7); each declares its own `Route`
// constants (or aliases of a constant declared in another domain file, for endpoints
// that share a wire resource) and is re-exported flat here so every existing
// `crate::routes::CONSTANT` path keeps resolving unchanged.
pub mod ac_compat;
pub mod account;
pub mod auth;
pub mod backup;
pub mod backup_access_points;
pub mod blacklist;
pub mod burst_reporters;
pub mod data_usage;
pub mod ddns;
pub mod devices;
pub mod dhcp;
pub mod diagnostics;
pub mod dns;
pub mod dns_policies;
pub mod eeros;
pub mod entitlements;
pub mod events;
pub mod forwards;
pub mod insights;
pub mod members;
pub mod networks;
pub mod notifications;
pub mod ouicheck;
pub mod permissions;
pub mod power_saving;
pub mod profiles;
pub mod reservations;
pub mod routing;
pub mod schedule;
pub mod security;
pub mod sqm;
pub mod subnets;
pub mod support;
pub mod thread;
pub mod transfer;
pub mod updates;
pub mod wan;
pub mod wpa3;

// The 14 modules new in v8.0.0 (`account`, `backup_access_points`, `ddns`, `dhcp`,
// `dns_policies`, `entitlements`, `events`, `members`, `notifications`, `permissions`,
// `power_saving`, `subnets`, `wan`, `wpa3`) are currently empty — no `Resource`/`Nested`
// constant is declared until the domain port (phase G) lands one. A glob `pub use` of an empty
// module is legal (it re-exports nothing) but triggers `unused_imports` until the first constant
// exists, so each is marked `#[allow(unused_imports)]` individually rather than widened to a
// blanket module-level allow that would also hide a real future regression.
pub use ac_compat::*;
#[allow(unused_imports)]
pub use account::*;
pub use auth::*;
pub use backup::*;
#[allow(unused_imports)]
pub use backup_access_points::*;
pub use blacklist::*;
pub use burst_reporters::*;
pub use data_usage::*;
#[allow(unused_imports)]
pub use ddns::*;
pub use devices::*;
#[allow(unused_imports)]
pub use dhcp::*;
pub use diagnostics::*;
pub use dns::*;
#[allow(unused_imports)]
pub use dns_policies::*;
pub use eeros::*;
#[allow(unused_imports)]
pub use entitlements::*;
#[allow(unused_imports)]
pub use events::*;
pub use forwards::*;
pub use insights::*;
#[allow(unused_imports)]
pub use members::*;
pub use networks::*;
#[allow(unused_imports)]
pub use notifications::*;
pub use ouicheck::*;
#[allow(unused_imports)]
pub use permissions::*;
#[allow(unused_imports)]
pub use power_saving::*;
pub use profiles::*;
pub use reservations::*;
pub use routing::*;
pub use schedule::*;
pub use security::*;
pub use sqm::*;
#[allow(unused_imports)]
pub use subnets::*;
pub use support::*;
pub use thread::*;
pub use transfer::*;
pub use updates::*;
#[allow(unused_imports)]
pub use wan::*;
#[allow(unused_imports)]
pub use wpa3::*;

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
        use super::{ACCOUNT, LOGIN, LOGIN_REFRESH, LOGIN_RESEND, LOGIN_VERIFY, LOGOUT};

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

    // -------------------------- exhaustive route inventory -----------------------------

    use super::{
        ACCOUNT, ADD_TO_BLACKLIST, CONFIGURE_BACKUP_NETWORK, CONFIGURE_SECURITY, CONFIGURE_SQM,
        CREATE_BURST_REPORTER, CREATE_FORWARD, CREATE_PROFILE, CREATE_RESERVATION, DELETE_FORWARD,
        DELETE_PROFILE, DELETE_RESERVATION, GET_AC_COMPAT, GET_BACKUP_NETWORK, GET_BACKUP_STATUS,
        GET_BLACKLIST, GET_BLOCKED_APPLICATIONS, GET_BURST_REPORTERS, GET_DATA_USAGE,
        GET_DATA_USAGE_RESOURCE, GET_DEVICE, GET_DEVICE_TRANSFER_STATS, GET_DEVICES,
        GET_DIAGNOSTICS, GET_DNS_SETTINGS, GET_EERO, GET_EEROS, GET_FORWARDS, GET_INSIGHTS,
        GET_LED_STATUS, GET_NETWORK, GET_NETWORKS, GET_NIGHTLIGHT, GET_OUICHECK,
        GET_PREMIUM_STATUS, GET_PROFILE, GET_PROFILE_DEVICES, GET_PROFILE_SCHEDULE, GET_PROFILES,
        GET_RESERVATIONS, GET_ROUTING, GET_SECURITY_SETTINGS, GET_SQM_SETTINGS, GET_SUPPORT,
        GET_THREAD, GET_TRANSFER_STATS, GET_UPDATES, LOGIN, LOGIN_REFRESH, LOGIN_RESEND,
        LOGIN_VERIFY, LOGOUT, PAUSE_DEVICE, PAUSE_PROFILE, PUT_NETWORK_SETTINGS, PUT_PROFILE,
        REBOOT_EERO, REBOOT_NETWORK, REMOVE_FROM_BLACKLIST, RENAME_PROFILE, REQUEST_SUPPORT,
        RUN_DIAGNOSTICS, RUN_INSIGHTS, RUN_OUICHECK, RUN_SPEED_TEST, SET_BACKUP_NETWORK,
        SET_BAND_STEERING, SET_BLOCKED_APPLICATIONS, SET_CUSTOM_DNS, SET_DEVICE_NICKNAME,
        SET_DNS_CACHING, SET_DNS_MODE, SET_GUEST_NETWORK, SET_IPV6, SET_IPV6_DNS, SET_LED,
        SET_LED_BRIGHTNESS, SET_NETWORK_NAME, SET_NIGHTLIGHT, SET_PROFILE_DEVICES,
        SET_PROFILE_SCHEDULE, SET_SQM_AUTO, SET_SQM_BANDWIDTH, SET_SQM_ENABLED, SET_THREAD,
        SET_UPNP, SET_WPA3, UPDATE_PROFILE_BLOCK_LIST, UPDATE_PROFILE_CONTENT_FILTER,
        UPDATE_RESERVATION,
    };
    /// One row per `Route` constant declared in this module. This is the drift tripwire for
    /// the whole crate: if Eero ever moves an endpoint, exactly one row here should fail and
    /// point straight at the constant to fix.
    struct Case {
        name: &'static str,
        route: &'static Route,
        method: Method,
        version: ApiVersion,
        params: &'static [(&'static str, &'static str)],
        rendered: &'static str,
    }

    const CASES: &[Case] = &[
        Case {
            name: "LOGIN",
            route: &LOGIN,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login",
        },
        Case {
            name: "LOGIN_VERIFY",
            route: &LOGIN_VERIFY,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/verify",
        },
        Case {
            name: "LOGIN_RESEND",
            route: &LOGIN_RESEND,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/resend",
        },
        Case {
            name: "LOGOUT",
            route: &LOGOUT,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/logout",
        },
        Case {
            name: "LOGIN_REFRESH",
            route: &LOGIN_REFRESH,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/refresh",
        },
        Case {
            name: "ACCOUNT",
            route: &ACCOUNT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/account",
        },
        Case {
            name: "GET_AC_COMPAT",
            route: &GET_AC_COMPAT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ac_compat",
        },
        Case {
            name: "GET_BACKUP_NETWORK",
            route: &GET_BACKUP_NETWORK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "GET_BACKUP_STATUS",
            route: &GET_BACKUP_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup/status",
        },
        Case {
            name: "SET_BACKUP_NETWORK",
            route: &SET_BACKUP_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "CONFIGURE_BACKUP_NETWORK",
            route: &CONFIGURE_BACKUP_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "GET_BLACKLIST",
            route: &GET_BLACKLIST,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist",
        },
        Case {
            name: "ADD_TO_BLACKLIST",
            route: &ADD_TO_BLACKLIST,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist",
        },
        Case {
            name: "REMOVE_FROM_BLACKLIST",
            route: &REMOVE_FROM_BLACKLIST,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("mac_or_device_id", "aabbccddeeff")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist/aabbccddeeff",
        },
        Case {
            name: "GET_BURST_REPORTERS",
            route: &GET_BURST_REPORTERS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/burst_reporters",
        },
        Case {
            name: "CREATE_BURST_REPORTER",
            route: &CREATE_BURST_REPORTER,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/burst_reporters",
        },
        Case {
            name: "GET_DATA_USAGE",
            route: &GET_DATA_USAGE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/data_usage",
        },
        Case {
            name: "GET_DATA_USAGE_RESOURCE",
            route: &GET_DATA_USAGE_RESOURCE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("resource", "devices")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/data_usage/devices",
        },
        Case {
            name: "GET_DEVICES",
            route: &GET_DEVICES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices",
        },
        Case {
            name: "GET_DEVICE",
            route: &GET_DEVICE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices/dev1",
        },
        Case {
            name: "SET_DEVICE_NICKNAME",
            route: &SET_DEVICE_NICKNAME,
            method: Method::PUT,
            version: ApiVersion::V2_3,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.3/networks/100/devices/dev1",
        },
        Case {
            name: "PAUSE_DEVICE",
            route: &PAUSE_DEVICE,
            method: Method::PUT,
            version: ApiVersion::V2_3,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.3/networks/100/devices/dev1",
        },
        Case {
            name: "GET_DIAGNOSTICS",
            route: &GET_DIAGNOSTICS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/diagnostics",
        },
        Case {
            name: "RUN_DIAGNOSTICS",
            route: &RUN_DIAGNOSTICS,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/diagnostics",
        },
        Case {
            name: "GET_DNS_SETTINGS",
            route: &GET_DNS_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_DNS_CACHING",
            route: &SET_DNS_CACHING,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_CUSTOM_DNS",
            route: &SET_CUSTOM_DNS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_DNS_MODE",
            route: &SET_DNS_MODE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_IPV6_DNS",
            route: &SET_IPV6_DNS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_EEROS",
            route: &GET_EEROS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/eeros",
        },
        Case {
            name: "GET_EERO",
            route: &GET_EERO,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_LED_STATUS",
            route: &GET_LED_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_NIGHTLIGHT",
            route: &GET_NIGHTLIGHT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "REBOOT_EERO",
            route: &REBOOT_EERO,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1/reboot",
        },
        Case {
            name: "SET_LED",
            route: &SET_LED,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "SET_LED_BRIGHTNESS",
            route: &SET_LED_BRIGHTNESS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "SET_NIGHTLIGHT",
            route: &SET_NIGHTLIGHT,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_FORWARDS",
            route: &GET_FORWARDS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards",
        },
        Case {
            name: "CREATE_FORWARD",
            route: &CREATE_FORWARD,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards",
        },
        Case {
            name: "DELETE_FORWARD",
            route: &DELETE_FORWARD,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("forward_id", "fwd1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards/fwd1",
        },
        Case {
            name: "GET_INSIGHTS",
            route: &GET_INSIGHTS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/insights",
        },
        Case {
            name: "RUN_INSIGHTS",
            route: &RUN_INSIGHTS,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/insights",
        },
        Case {
            name: "GET_NETWORKS",
            route: &GET_NETWORKS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/networks",
        },
        Case {
            name: "GET_NETWORK",
            route: &GET_NETWORK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "GET_PREMIUM_STATUS",
            route: &GET_PREMIUM_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_GUEST_NETWORK",
            route: &SET_GUEST_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/guestnetwork",
        },
        Case {
            name: "RUN_SPEED_TEST",
            route: &RUN_SPEED_TEST,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/speedtest",
        },
        Case {
            name: "REBOOT_NETWORK",
            route: &REBOOT_NETWORK,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reboot",
        },
        Case {
            name: "PUT_NETWORK_SETTINGS",
            route: &PUT_NETWORK_SETTINGS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_NETWORK_NAME",
            route: &SET_NETWORK_NAME,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_OUICHECK",
            route: &GET_OUICHECK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ouicheck",
        },
        Case {
            name: "RUN_OUICHECK",
            route: &RUN_OUICHECK,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ouicheck",
        },
        Case {
            name: "GET_PROFILES",
            route: &GET_PROFILES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles",
        },
        Case {
            name: "CREATE_PROFILE",
            route: &CREATE_PROFILE,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles",
        },
        Case {
            name: "GET_PROFILE",
            route: &GET_PROFILE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "PUT_PROFILE",
            route: &PUT_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "PAUSE_PROFILE",
            route: &PAUSE_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_PROFILE_DEVICES",
            route: &GET_PROFILE_DEVICES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_PROFILE_DEVICES",
            route: &SET_PROFILE_DEVICES,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "UPDATE_PROFILE_CONTENT_FILTER",
            route: &UPDATE_PROFILE_CONTENT_FILTER,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "UPDATE_PROFILE_BLOCK_LIST",
            route: &UPDATE_PROFILE_BLOCK_LIST,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_BLOCKED_APPLICATIONS",
            route: &GET_BLOCKED_APPLICATIONS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_BLOCKED_APPLICATIONS",
            route: &SET_BLOCKED_APPLICATIONS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "RENAME_PROFILE",
            route: &RENAME_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "DELETE_PROFILE",
            route: &DELETE_PROFILE,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_RESERVATIONS",
            route: &GET_RESERVATIONS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations",
        },
        Case {
            name: "CREATE_RESERVATION",
            route: &CREATE_RESERVATION,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations",
        },
        Case {
            name: "UPDATE_RESERVATION",
            route: &UPDATE_RESERVATION,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("reservation_id", "res1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations/res1",
        },
        Case {
            name: "DELETE_RESERVATION",
            route: &DELETE_RESERVATION,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("reservation_id", "res1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations/res1",
        },
        Case {
            name: "GET_ROUTING",
            route: &GET_ROUTING,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/routing",
        },
        Case {
            name: "GET_PROFILE_SCHEDULE",
            route: &GET_PROFILE_SCHEDULE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_PROFILE_SCHEDULE",
            route: &SET_PROFILE_SCHEDULE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_SECURITY_SETTINGS",
            route: &GET_SECURITY_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_WPA3",
            route: &SET_WPA3,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_BAND_STEERING",
            route: &SET_BAND_STEERING,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_UPNP",
            route: &SET_UPNP,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_IPV6",
            route: &SET_IPV6,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_THREAD",
            route: &SET_THREAD,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "CONFIGURE_SECURITY",
            route: &CONFIGURE_SECURITY,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_SQM_SETTINGS",
            route: &GET_SQM_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_SQM_ENABLED",
            route: &SET_SQM_ENABLED,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_SQM_BANDWIDTH",
            route: &SET_SQM_BANDWIDTH,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "CONFIGURE_SQM",
            route: &CONFIGURE_SQM,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_SQM_AUTO",
            route: &SET_SQM_AUTO,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_SUPPORT",
            route: &GET_SUPPORT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/support",
        },
        Case {
            name: "REQUEST_SUPPORT",
            route: &REQUEST_SUPPORT,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/support",
        },
        Case {
            name: "GET_THREAD",
            route: &GET_THREAD,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/thread",
        },
        Case {
            name: "GET_TRANSFER_STATS",
            route: &GET_TRANSFER_STATS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/transfer",
        },
        Case {
            name: "GET_DEVICE_TRANSFER_STATS",
            route: &GET_DEVICE_TRANSFER_STATS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices/dev1/transfer",
        },
        Case {
            name: "GET_UPDATES",
            route: &GET_UPDATES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/updates",
        },
    ];

    #[test]
    fn every_domain_route_constant_has_the_expected_verb_version_and_rendered_path() {
        for case in CASES {
            assert_eq!(
                case.route.method, case.method,
                "{}: unexpected method",
                case.name
            );
            assert_eq!(
                case.route.version, case.version,
                "{}: unexpected version",
                case.name
            );
            let url = case
                .route
                .render(case.params)
                .unwrap_or_else(|e| panic!("{}: failed to render: {e}", case.name));
            assert_eq!(
                url.as_str(),
                case.rendered,
                "{}: unexpected rendered URL",
                case.name
            );
        }
    }

    // ------------------------------- API-version invariants -----------------------------

    #[test]
    fn device_mutation_routes_are_v2_3_while_a_representative_read_route_is_v2_2() {
        use super::{GET_DEVICES, PAUSE_DEVICE, SET_DEVICE_NICKNAME};

        // Device nickname/pause writes must go to 2.3 — 2.2 accepts them, returns 200, and
        // silently drops the change (eero-api issue #102). Getting this wrong is silent data
        // loss, so this is asserted explicitly rather than only covered by the table above.
        assert_eq!(SET_DEVICE_NICKNAME.version, ApiVersion::V2_3);
        assert_eq!(PAUSE_DEVICE.version, ApiVersion::V2_3);

        // A representative read (device list) stays on the 2.2 default.
        assert_eq!(GET_DEVICES.version, ApiVersion::V2_2);
    }

    // ------------------------------ path template hygiene -------------------------------

    #[test]
    fn no_path_template_is_absolute_or_hardcodes_a_version_or_host() {
        // Every `Route::path` must be a bare, relative, `/`-separated template: the leading
        // slash, the "2.2"/"2.3" version segment and the host all come from
        // `ApiVersion::base_url`, never from the template itself. A template that embedded any
        // of these would double up on render (e.g. "//2.2/...") or silently ignore the
        // `ApiVersion` the constant declares.
        for case in CASES {
            let path = case.route.path;
            assert!(
                !path.starts_with('/'),
                "{}: path template must not start with '/' (leading slash comes from the base URL)",
                case.name
            );
            assert!(
                !path.contains("2.2") && !path.contains("2.3"),
                "{}: path template must not hardcode an API version (that's `ApiVersion`'s job)",
                case.name
            );
            assert!(
                !path.contains("e2ro.com") && !path.contains("http"),
                "{}: path template must not hardcode the host (that's `ApiVersion::base_url`'s job)",
                case.name
            );
        }
    }

    // ------------------------- validate_segment / traversal rejection -------------------

    use super::{SegmentError, validate_segment};

    #[test]
    fn validate_segment_rejects_empty_value() {
        assert_eq!(validate_segment(""), Err(SegmentError::Empty));
    }

    #[test]
    fn validate_segment_rejects_control_characters() {
        assert_eq!(
            validate_segment("..\n"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment(".\t."),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("\t.."),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("..\t"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("..\r\n"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("\u{0}"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("a\u{7f}b"),
            Err(SegmentError::ControlCharacter)
        );
    }

    #[test]
    fn validate_segment_rejects_literal_dot_segments() {
        assert_eq!(validate_segment("."), Err(SegmentError::DotSegment));
        assert_eq!(validate_segment(".."), Err(SegmentError::DotSegment));
    }

    #[test]
    fn validate_segment_accepts_legitimate_and_already_safe_values() {
        for value in [
            "device-0001",
            "aa:bb:cc:00:00:01",
            "a/b",
            "%2e%2e",
            "..%2f..",
            "．．", // fullwidth dots (U+FF0E) — not ASCII '.', never trips dot-segment removal
            "a?b",
            "a#b",
        ] {
            assert_eq!(
                validate_segment(value),
                Ok(()),
                "value {value:?} must be accepted"
            );
        }
    }

    #[test]
    fn render_rejects_a_hostile_placeholder_value_before_building_the_url() {
        // The concrete reproduction from the security finding: a trailing-newline id must not
        // collapse `.../networks/{network_id}/blacklist/{mac_or_device_id}` onto
        // `.../networks/{network_id}/blacklist/`.
        let route = Route {
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/blacklist/{mac_or_device_id}",
        };

        let err = route
            .render(&[("network_id", "100"), ("mac_or_device_id", "..\n")])
            .expect_err("a value that becomes \"..\" after tab/CR/LF stripping must be rejected");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::ControlCharacter,
            }
        );
    }

    #[test]
    fn render_rejects_an_empty_placeholder_value() {
        let route = Route {
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/profiles/{profile_id}",
        };

        let err = route
            .render(&[("network_id", "100"), ("profile_id", "")])
            .expect_err("an empty value must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "profile_id".to_owned(),
                reason: SegmentError::Empty,
            }
        );
    }

    /// Security review finding F1: a real destructive route
    /// ([`super::REMOVE_FROM_BLACKLIST`]), not a hand-built fixture, must reject a literal
    /// `".."` placeholder value rather than silently rendering
    /// `DELETE /2.2/networks/100/blacklist` (the whole collection) instead of one item. See
    /// `tests/path_safety.rs` for the same guarantee proven end to end, with a mock server
    /// verifying zero requests are ever sent.
    #[test]
    fn remove_from_blacklist_rejects_a_literal_dot_dot_id() {
        let err = super::REMOVE_FROM_BLACKLIST
            .render(&[("network_id", "100"), ("mac_or_device_id", "..")])
            .expect_err("a literal \"..\" id must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::DotSegment,
            }
        );
    }

    /// Same as [`remove_from_blacklist_rejects_a_literal_dot_dot_id`], for a bare `"."`.
    #[test]
    fn remove_from_blacklist_rejects_a_literal_single_dot_id() {
        let err = super::REMOVE_FROM_BLACKLIST
            .render(&[("network_id", "100"), ("mac_or_device_id", ".")])
            .expect_err("a literal \".\" id must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::DotSegment,
            }
        );
    }

    // ============================== Resource / Nested (v8 route model) ==============================

    use super::{Nested, Resource};
    use url::Url;

    fn fake_host() -> Url {
        Url::parse("https://api-user.e2ro.com").unwrap()
    }

    #[test]
    fn resource_with_placeholder_resolves_bare_id_against_its_version() {
        let resource = Resource {
            method: Method::GET,
            version: ApiVersion::V2_2,
            template: "networks/{id}",
            link: None,
        };
        let url = resource.resolve(&fake_host(), "100", None).unwrap();
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/networks/100");
    }

    #[test]
    fn resource_without_placeholder_resolves_a_fixed_path_and_ignores_id_or_url() {
        let resource = Resource {
            method: Method::GET,
            version: ApiVersion::V2_2,
            template: "account",
            link: None,
        };
        let url = resource.resolve(&fake_host(), "ignored", None).unwrap();
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/account");
    }

    #[test]
    fn resource_with_link_prefers_the_parents_published_link() {
        let resource = Resource {
            method: Method::PUT,
            version: ApiVersion::V2_2,
            template: "networks/{id}/settings",
            link: Some("settings"),
        };
        let parent = serde_json::json!({"resources": {"settings": "/2.4/networks/100/settings"}});
        let url = resource
            .resolve(&fake_host(), "100", Some(&parent))
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.4/networks/100/settings"
        );
    }

    #[test]
    fn resource_with_link_falls_back_when_no_parent_supplied() {
        let resource = Resource {
            method: Method::PUT,
            version: ApiVersion::V2_2,
            template: "networks/{id}/settings",
            link: Some("settings"),
        };
        let url = resource.resolve(&fake_host(), "100", None).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/settings"
        );
    }

    #[test]
    fn nested_resolves_bare_child_id() {
        let nested = Nested {
            method: Method::GET,
            version: ApiVersion::V2_2,
            prefix: "profiles",
            suffix: "",
            link: None,
        };
        let url = nested.resolve(&fake_host(), "100", "p1", None).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/profiles/p1"
        );
    }

    #[test]
    fn nested_with_link_prefers_the_parents_published_link() {
        let nested = Nested {
            method: Method::GET,
            version: ApiVersion::V2_2,
            prefix: "profiles",
            suffix: "/schedules",
            link: Some("schedules"),
        };
        let parent = serde_json::json!({"resources": {"schedules": "/2.5/networks/100/profiles/p1/schedules"}});
        let url = nested
            .resolve(&fake_host(), "100", "p1", Some(&parent))
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.5/networks/100/profiles/p1/schedules"
        );
    }
}
