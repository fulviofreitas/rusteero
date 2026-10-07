//! Every wire endpoint as a [`Resource`] or [`Nested`] constant.
//!
//! Endpoint modules never build URLs by hand; upstream drift is a one-line fix here.
//!
//! There is no Python original for this module: `eero-api` builds each URL inline, string by
//! string, in every `api/*.py` file (e.g. `f"{API_ENDPOINT}/networks/{network_id}/devices"`).
//! `rusteero` centralises the same information as data instead: a [`Resource`] pairs an HTTP
//! [`Method`], an [`ApiVersion`] (which selects the base host), a path template with at most one
//! `{id}` placeholder, and an optional published-link name; a [`Nested`] is the two-level
//! `networks/{network}/{prefix}/{child}{suffix}` shape. Both resolve against an explicit `host`
//! `Url` (see each type's own `resolve` docs) via [`crate::links`]/[`crate::params`], so a
//! server-side path rename or version bump is a single-line edit with one wiremock test to match,
//! instead of a grep-and-replace across two dozen endpoint modules.
//!
//! ## Path traversal
//!
//! Every identifier substituted into a resolved URL is validated before it reaches the request:
//! [`crate::links::validate_identifier`] (used by [`crate::links::resource_url`]/`child_url`, and
//! therefore by every [`Resource`]/[`Nested`] resolution with a `{id}` placeholder) enforces a
//! strict single-path-segment whitelist (`^[A-Za-z0-9][A-Za-z0-9._:-]*$`, plus an explicit `".."`
//! rejection) — an empty, `"."`, `".."`, control-character-bearing, or separator-bearing value is
//! rejected outright, before any request is sent, rather than silently percent-encoded or
//! (worse) collapsed onto a parent collection. `validate_segment`/`SegmentError` below are a
//! second, narrower rule used only where a candidate identifier is *not* going through
//! `resource_url`/`child_url` — currently [`crate::client`]'s network-id auto-discovery
//! (`extract_network_id`), which reads a candidate id straight out of an untrusted `/networks`
//! response body.

use crate::error::Error;
use reqwest::Method;
use serde_json::Value;
use url::Url;

/// Selects which Eero cloud API host/version a [`Resource`]/[`Nested`] targets.
///
/// Ported from the fact — not the code shape — of `const.py:7,13`: `eero-api` hardcodes two
/// base URLs (`API_ENDPOINT` for almost everything, `DEVICE_UPDATE_ENDPOINT` for device
/// nickname/pause writes only). `rusteero` keeps the same two hosts but names them as an enum so
/// every [`Resource`]/[`Nested`] states its version explicitly instead of picking a base-URL
/// constant by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiVersion {
    /// API version 2.2 — the default for almost every endpoint.
    ///
    /// Base URL: [`crate::consts::API_BASE_22`].
    V2_2,
    /// API version 2.3 — required for device-mutation writes (nickname, pause); see
    /// [`crate::consts::API_BASE_23`] for why 2.2 cannot be used for those instead.
    V2_3,
}

impl ApiVersion {
    /// Returns the bare version path segment for this API version (`"2.2"` or `"2.3"`), the
    /// single source of truth every `links`/`params` caller derives its version string from, so a
    /// future version bump only ever needs one edit.
    #[must_use]
    pub const fn segment(self) -> &'static str {
        match self {
            Self::V2_2 => crate::consts::API_VERSION_DEFAULT,
            Self::V2_3 => crate::consts::API_VERSION_DEVICE_WRITES,
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

/// Why `validate_segment` rejected a value before it could be substituted into a rendered
/// path segment.
///
/// See the module docs' "Path traversal" section for the vulnerability this closes, exactly what
/// each variant means, and the (narrower) case `validate_segment` is still used for today —
/// most identifier validation in this crate goes through [`crate::links::validate_identifier`]
/// instead.
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
/// Used directly by [`crate::client`]'s network-id auto-discovery (`extract_network_id`), which
/// reads a candidate network id straight out of an untrusted `/networks` response body before any
/// [`Resource`]/[`Nested`] resolution ever sees it — every other identifier
/// in this crate is validated by [`crate::links::validate_identifier`] instead, at the point it is
/// substituted into a resolved URL. Validating the *raw* value here, rather than trusting the
/// `url` crate's own encoder to make any value safe, is deliberate: the encoder strips ASCII
/// tab/CR/LF bytes from a segment *before* percent-encoding and dot-segment removal run, so a
/// value that is not literally `".."` can still be treated as `".."` by the time it is written
/// into a URL. See the module docs' "Path traversal" section for the full mechanism and a
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

// =====================================================================================
// Domain routes. Every wire endpoint from the Python `eero-api` package gets one
// `Resource`/`Nested` constant per unique (verb, version, path) combination. Several
// Python methods across different modules turn out to hit the exact same wire endpoint
// (same verb, version and path) — those are modelled as an alias constant (`pub const
// ALIAS: Resource = CANONICAL;`) rather than a second literal, so a server-side path
// change is still a one-line fix.
//
// Deliberately NOT ported here (see this crate's lessons
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

// One file per Python domain module; each declares its own `Resource`/
// `Nested` constants (or aliases of a constant declared in another domain file, for
// endpoints that share a wire resource) and is re-exported flat here so every existing
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
// `power_saving`, `subnets`, `wan`, `wpa3`) each declare their own `Resource`/`Nested`
// constants, re-exported below via `pub use`.
pub use ac_compat::*;
pub use account::*;
pub use auth::*;
pub use backup::*;
pub use backup_access_points::*;
pub use blacklist::*;
pub use burst_reporters::*;
pub use data_usage::*;
pub use ddns::*;
pub use devices::*;
pub use dhcp::*;
pub use diagnostics::*;
pub use dns::*;
pub use dns_policies::*;
pub use eeros::*;
pub use entitlements::*;
pub use events::*;
pub use forwards::*;
pub use insights::*;
pub use members::*;
pub use networks::*;
pub use notifications::*;
// `ouicheck` deliberately declares no `Resource`/`Nested` constant at all — see that module's
// own docs for why — so this glob re-export legitimately re-exports nothing.
#[allow(unused_imports)]
pub use ouicheck::*;
pub use permissions::*;
pub use power_saving::*;
pub use profiles::*;
pub use reservations::*;
pub use routing::*;
pub use schedule::*;
pub use security::*;
pub use sqm::*;
pub use subnets::*;
pub use support::*;
pub use thread::*;
pub use transfer::*;
pub use updates::*;
pub use wan::*;
pub use wpa3::*;

#[cfg(test)]
mod tests {
    use super::ApiVersion;
    use reqwest::Method;

    // ------------------------------- required routes ---------------------------------

    #[test]
    fn required_auth_and_account_routes_have_the_expected_verb_version_and_template() {
        use super::{ACCOUNT, LOGIN, LOGIN_REFRESH, LOGIN_RESEND, LOGIN_VERIFY, LOGOUT};

        assert_eq!(LOGIN.method, Method::POST);
        assert_eq!(LOGIN.version, ApiVersion::V2_2);
        assert_eq!(LOGIN.template, "login");

        assert_eq!(LOGIN_VERIFY.method, Method::POST);
        assert_eq!(LOGIN_VERIFY.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_VERIFY.template, "login/verify");

        assert_eq!(LOGIN_RESEND.method, Method::POST);
        assert_eq!(LOGIN_RESEND.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_RESEND.template, "login/resend");

        assert_eq!(LOGOUT.method, Method::POST);
        assert_eq!(LOGOUT.version, ApiVersion::V2_2);
        assert_eq!(LOGOUT.template, "logout");

        assert_eq!(LOGIN_REFRESH.method, Method::POST);
        assert_eq!(LOGIN_REFRESH.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_REFRESH.template, "login/refresh");

        assert_eq!(ACCOUNT.method, Method::GET);
        assert_eq!(ACCOUNT.version, ApiVersion::V2_2);
        assert_eq!(ACCOUNT.template, "account");
    }

    #[test]
    fn login_and_verify_resolve_to_the_expected_absolute_urls() {
        use super::{LOGIN, LOGIN_VERIFY};
        use url::Url;

        let host = Url::parse("https://api-user.e2ro.com").unwrap();
        assert_eq!(
            LOGIN.resolve(&host, "", None).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login"
        );
        assert_eq!(
            LOGIN_VERIFY.resolve(&host, "", None).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login/verify"
        );
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
