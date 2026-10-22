//! Resource-link resolution for Eero API response envelopes.
//!
//! Ported from `eero-api`'s `src/eero/api/links.py` at `v8.0.4`. The API returns
//! hypermedia-style links inside response envelopes: a resource's own `url` and, for several
//! resource types, a `resources` object whose values are host-relative paths to related
//! resources (for example a network's `eeros`, `devices`, or `guestnetwork` links). This module
//! is the single place in the crate that turns those links into absolute URLs.
//!
//! This module performs no I/O and never mutates the envelopes it reads — it only reads fields
//! and returns [`Url`]s. Every helper here takes the configured API host as an explicit `&Url`
//! parameter (scheme + host, no path) rather than reading a crate-global constant, so a test can
//! point every helper at a wiremock server instead of the real Eero cloud host.

use serde_json::Value;
use url::Url;

use crate::error::Error;

/// The single placeholder every URL template carries for the resource id.
const ID_PLACEHOLDER: &str = "{id}";

/// Validates a bare resource identifier before it is placed in a path.
///
/// Ported from `_validate_identifier`/`validate_identifier` (`links.py:50-84`): the identifier
/// regex `^[A-Za-z0-9][A-Za-z0-9._:-]*$`, plus a literal `".."` rejection (so `"a..b"` is
/// rejected despite matching the regex, since it would still traverse the URL path if joined
/// unescaped).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "id"` if `value` is empty, contains a path or
/// query delimiter, a percent-escape, whitespace, or a `".."` sequence.
pub fn validate_identifier(value: &str) -> Result<&str, Error> {
    let mut chars = value.chars();
    let starts_valid = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_valid = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'));
    if !starts_valid || !rest_valid || value.contains("..") {
        return Err(Error::validation(
            "id",
            "must be a single path segment identifier",
        ));
    }
    Ok(value)
}

/// Validates a link value read from an envelope before joining it onto the API host.
///
/// Ported from `_validate_link_path` (`links.py:90-104`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "link"` if `link` is empty, contains `"://"`, or
/// starts with `"//"` — none of which an API-published path ever does. Also rejects a query
/// string or fragment (security finding 3): every caller of this helper joins `link` verbatim
/// onto the API host via [`join_api_path`], and a query/fragment slipped through here would ride
/// along into the resolved request URL unexamined, whether `link` came from an envelope's
/// `resources`/`url` field ([`resolve_link`]/[`self_url`]) or from a caller-supplied
/// `/`-prefixed `id_or_url` ([`resource_url`]'s path branch).
fn validate_link_path(link: &str) -> Result<&str, Error> {
    if link.is_empty() || link.contains("://") || link.starts_with("//") {
        return Err(Error::validation(
            "link",
            "must be a host-relative API path",
        ));
    }
    if link.contains(['?', '#']) {
        return Err(Error::validation(
            "link",
            "must not carry a query or fragment",
        ));
    }
    Ok(link)
}

/// Returns the `data` object of `parent`, without copying it, when `parent` looks like a full
/// response envelope (has both a `meta` key and an object `data` key); otherwise returns `parent`
/// unchanged.
///
/// Ported from `_as_data` (`links.py:108-124`). `pub(crate)` so other modules that need the
/// identical "is this a full envelope or an already-unwrapped `data` object?" rule (e.g.
/// `crate::endpoints::eeros`'s nightlight-URL reader) share this one implementation instead of
/// reimplementing — and potentially loosening — the same check.
pub(crate) fn as_data(parent: &Value) -> &Value {
    if let Some(object) = parent.as_object()
        && object.contains_key("meta")
        && object.get("data").is_some_and(Value::is_object)
    {
        return &object["data"];
    }
    parent
}

/// Resolves a host-relative API path to an absolute [`Url`] on `host`.
///
/// Ported from `join_api_path` (`links.py:127-144`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "path"` if `path` is empty.
pub fn join_api_path(host: &Url, path: &str) -> Result<Url, Error> {
    if path.is_empty() {
        return Err(Error::validation("path", "must be a non-empty string"));
    }
    let host_str = host.as_str().trim_end_matches('/');
    let joined = format!("{host_str}/{}", path.trim_start_matches('/'));
    Url::parse(&joined).map_err(|err| Error::validation("path", format!("not a valid URL: {err}")))
}

/// Validates that an absolute URL is on `host` (case-insensitive scheme + hostname match).
///
/// Ported from `_validate_absolute_url` (`links.py:146-174`). Rejects userinfo tricks (e.g.
/// `https://api-user.e2ro.com@evil.example/...`, where the real host is `evil.example`) and
/// suffix tricks (e.g. `https://api-user.e2ro.com.evil.example/...`) since [`Url::host_str`]
/// resolves to the real host in both cases, exactly like Python's `urlsplit(...).hostname`.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "url"` if `url` does not parse, its scheme or
/// hostname does not exactly match `host`'s, or it carries a query string or fragment (security
/// finding 3: every caller of this helper appends a template-derived suffix onto the validated
/// URL and sends the result as a request URL, so a query/fragment slipped through here would
/// ride along unexamined).
fn validate_absolute_url(url: &str, host: &Url) -> Result<Url, Error> {
    let parsed = Url::parse(url)
        .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))?;
    let host_matches = matches!(
        (parsed.host_str(), host.host_str()),
        (Some(a), Some(b)) if a.eq_ignore_ascii_case(b)
    );
    let scheme_matches = parsed.scheme().eq_ignore_ascii_case(host.scheme());
    if !(host_matches && scheme_matches) {
        return Err(Error::validation(
            "url",
            format!(
                "must be an absolute {} URL on {}",
                host.scheme(),
                host.host_str().unwrap_or_default()
            ),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(Error::validation(
            "url",
            "must not carry a query or fragment",
        ));
    }
    Ok(parsed)
}

/// Resolves a named resource link from a parent envelope to an absolute [`Url`].
///
/// Reads `parent["resources"][name]` (or `parent["data"]["resources"][name]` when `parent` is a
/// full envelope) and joins it onto `host`. Ported from `resolve_link` (`links.py:176-203`).
///
/// # Errors
///
/// Returns [`Error::validation`] if the link value read from the envelope is malformed (must not
/// contain `"://"` or start with `"//"`). Never errors for a *missing* link — that returns
/// `Ok(None)`.
pub fn resolve_link(host: &Url, parent: &Value, name: &str) -> Result<Option<Url>, Error> {
    let data = as_data(parent);
    let Some(resources) = data.get("resources").and_then(Value::as_object) else {
        return Ok(None);
    };
    let Some(link) = resources.get(name).and_then(Value::as_str) else {
        return Ok(None);
    };
    if link.is_empty() {
        return Ok(None);
    }
    join_api_path(host, validate_link_path(link)?).map(Some)
}

/// Resolves a parent envelope's own `url` link to an absolute [`Url`].
///
/// Ported from `self_url` (`links.py:205-221`).
///
/// # Errors
///
/// Returns [`Error::validation`] if the `url` value read from the envelope is malformed. Never
/// errors when no `url` field is present — that returns `Ok(None)`.
pub fn self_url(host: &Url, parent: &Value) -> Result<Option<Url>, Error> {
    let data = as_data(parent);
    let Some(url) = data.get("url").and_then(Value::as_str) else {
        return Ok(None);
    };
    if url.is_empty() {
        return Ok(None);
    }
    join_api_path(host, validate_link_path(url)?).map(Some)
}

/// Resolves a bare ID or an API-returned path/URL to an absolute [`Url`], the single helper for
/// the id-or-url polymorphism domain modules encounter throughout the API.
///
/// `template` must contain exactly one `{id}` placeholder, e.g. `"networks/{id}/settings"`.
/// Interpreted as relative to `version`.
///
/// Ported from `resource_url` (`links.py:223-278`):
/// - An absolute `http(s)://` `id_or_url` is validated on-`host`, then `.rstrip('/') + suffix`
///   (the template text after `{id}`) is appended.
/// - A `/`-prefixed `id_or_url` is joined via [`join_api_path`], with the same suffix appended.
/// - A bare identifier is validated and substituted into the whole template, resolved against
///   `host/{version}`.
///
/// # Errors
///
/// Returns [`Error::validation`] if `id_or_url` is empty, `template` does not contain exactly
/// one `{id}` placeholder, or the id/path/URL itself fails validation.
pub fn resource_url(
    host: &Url,
    id_or_url: &str,
    template: &str,
    version: crate::routes::ApiVersion,
) -> Result<Url, Error> {
    if id_or_url.is_empty() {
        return Err(Error::validation("id_or_url", "must be a non-empty string"));
    }
    if template.matches(ID_PLACEHOLDER).count() != 1 {
        return Err(Error::validation(
            "template",
            "must contain exactly one {id} placeholder",
        ));
    }
    let Some(idx) = template.find(ID_PLACEHOLDER) else {
        // Unreachable: the `matches(..).count() != 1` check above already returned `Err` for
        // zero occurrences, so a single occurrence is always found here. Written as a second
        // real check (rather than `.expect(..)`) so this function can never panic, keeping the
        // "every fallible operation returns `Result`" convention exact.
        return Err(Error::validation(
            "template",
            "must contain exactly one {id} placeholder",
        ));
    };
    let suffix = &template[idx + ID_PLACEHOLDER.len()..];

    let lower = id_or_url.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let validated = validate_absolute_url(id_or_url, host)?;
        let joined = format!("{}{suffix}", validated.as_str().trim_end_matches('/'));
        return Url::parse(&joined)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")));
    }

    if id_or_url.starts_with('/') {
        let joined = join_api_path(host, validate_link_path(id_or_url)?)?;
        let with_suffix = format!("{}{suffix}", joined.as_str().trim_end_matches('/'));
        return Url::parse(&with_suffix)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")));
    }

    let id = validate_identifier(id_or_url)?;
    let path = template.replacen(ID_PLACEHOLDER, id, 1);
    // Resolve against the *configured* host, never the production constant: `host` is the
    // transport's `api_host()` (a wiremock server in tests, a caller-supplied `base_url` in
    // production), exactly like the absolute-URL and path branches above and
    // `Resource::resolve`'s fixed-path branch. Python's `resource_url` reads the global
    // `api_endpoint(version)` (`links.py:277`), which is the same thing there because Python
    // has no per-transport host override.
    let base = host.as_str().trim_end_matches('/');
    let full = format!(
        "{base}/{}/{}",
        version.segment(),
        path.trim_start_matches('/')
    );
    Url::parse(&full).map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
}

/// Appends a validated child identifier to an already-resolved URL.
///
/// Ported from `child_url` (`links.py:280-298`).
///
/// # Errors
///
/// Returns [`Error::validation`] if `child_id` is not a single path segment identifier.
pub fn child_url(base: &Url, child_id: &str) -> Result<Url, Error> {
    let id = validate_identifier(child_id)?;
    let joined = format!("{}/{id}", base.as_str().trim_end_matches('/'));
    Url::parse(&joined).map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
}

/// Resolves a sub-resource URL, preferring the parent's own published link.
///
/// When `parent` is supplied and [`resolve_link`] finds a link named `link` on it, that link is
/// used verbatim (including whatever version prefix the API published); otherwise the URL is
/// built from `id_or_url` and `template` exactly as [`resource_url`] does.
///
/// Ported from `sub_resource_url` (`links.py:301-333`).
///
/// # Errors
///
/// Returns [`Error::validation`] as [`resolve_link`] or [`resource_url`].
pub fn sub_resource_url(
    host: &Url,
    id_or_url: &str,
    template: &str,
    link: &str,
    parent: Option<&Value>,
    version: crate::routes::ApiVersion,
) -> Result<Url, Error> {
    if let Some(parent) = parent
        && let Some(resolved) = resolve_link(host, parent, link)?
    {
        return Ok(resolved);
    }
    resource_url(host, id_or_url, template, version)
}

/// Returns `true` when `segment` is a bare API-version path segment: ASCII digits, a single
/// `.`, ASCII digits (`2.2`, `2.3`, `2.10`).
///
/// Ported from `_VERSION_SEGMENT_RE = [0-9]+\.[0-9]+` used with `fullmatch` (`links.py:396`).
/// `char::is_ascii_digit` rather than `char::is_numeric`, so non-ASCII digits never qualify, and
/// a second dot (`2.2.2`) is rejected exactly as the anchored regex rejects it. Shared by
/// [`rewrite_version`] and the nested-family path matchers in [`crate::params`], so the two agree
/// on what a version segment is.
pub(crate) fn is_version_segment(segment: &str) -> bool {
    let Some((major, minor)) = segment.split_once('.') else {
        return false;
    };
    !major.is_empty()
        && !minor.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && minor.chars().all(|c| c.is_ascii_digit())
}

/// Rewrites the API-version segment of an already-resolved absolute URL.
///
/// Used where a domain method must pin a specific API version regardless of which version
/// segment a parent envelope's published link named, because the response shape (or whether a
/// write takes effect at all) differs by version. Resolve the URL first, rewrite it, then
/// validate and send it.
///
/// Ported from `rewrite_version` (`links.py:399-435`, the parsed form), with its tests.
///
/// The version is the **first path segment** of a resolved URL (the configured API host is a bare
/// origin), and only that segment is replaced: `/2.3/networks/n/2.3/support` pinned to
/// [`ApiVersion::V2_2`] becomes `/2.2/networks/n/2.3/support`. Everything else is left exactly
/// as `url` holds it: scheme, host, port, the rest of the path with its percent-escapes, the
/// query and the fragment.
///
/// The function never fails. `url` is returned unchanged (as a clone) when any of these holds:
///
/// - the scheme is not `http` or `https` (for example `ftp://host/2.3/x/y`);
/// - the URL has no host;
/// - the path has no `/` after its first segment (`/2.3` and the bare `/` are left alone, while
///   `/2.3/` is rewritten to `/2.2/`, as Python's `path.split("/", 2)` requires three parts);
/// - the first path segment is not `<digits>.<digits>` in ASCII (`/v2/x`, `/2/x`, `/2.2.2/x`,
///   `/login/x`).
///
/// Python's relative-path case has no counterpart here: a [`Url`] is always absolute. The two
/// hardening cases upstream tests for need no code either: [`Url::parse`] already lower-cases the
/// scheme (`HTTPS://` pins like `https://`) and drops tab, carriage return and line feed from the
/// input, while any other control character in the path is percent-encoded rather than left to
/// hide the version segment.
///
/// # Examples
///
/// ```
/// use rusteero::links::rewrite_version;
/// use rusteero::routes::ApiVersion;
/// use url::Url;
///
/// let url = Url::parse("http://mock.test:1234/2.3/networks/n1/support?a=1").unwrap();
/// let pinned = rewrite_version(&url, ApiVersion::V2_2);
/// assert_eq!(pinned.as_str(), "http://mock.test:1234/2.2/networks/n1/support?a=1");
/// ```
#[must_use]
pub fn rewrite_version(url: &Url, version: crate::routes::ApiVersion) -> Url {
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return url.clone();
    }
    let Some((segment, tail)) = url.path().strip_prefix('/').and_then(|p| p.split_once('/')) else {
        return url.clone();
    };
    if !is_version_segment(segment) {
        return url.clone();
    }
    // `set_path` keeps the query and the fragment, and re-parsing a path that `Url` itself
    // produced neither double-encodes an existing `%xx` escape nor changes dot segments (those
    // were already resolved when `url` was parsed).
    let mut rewritten = url.clone();
    rewritten.set_path(&format!("/{}/{tail}", version.segment()));
    rewritten
}

/// Reason reported by [`origin_mismatch`] when the URL's host is not the base URL's host (or the
/// URL has no host at all).
///
/// Ported from `ORIGIN_FOREIGN_HOST` (`base.py:217`). All `ORIGIN_*` phrases are short and fixed
/// because they are logged: none may ever carry a value taken from a URL.
pub const ORIGIN_FOREIGN_HOST: &str = "foreign host";

/// Reason reported by [`origin_mismatch`] when the URL's scheme is not the base URL's scheme.
///
/// Ported from `ORIGIN_SCHEME` (`base.py:218`).
pub const ORIGIN_SCHEME: &str = "non-matching scheme";

/// Reason reported by [`origin_mismatch`] when the URL's effective port is not the base URL's.
///
/// Ported from `ORIGIN_PORT` (`base.py:219`).
pub const ORIGIN_PORT: &str = "non-matching port";

/// Reason reported by [`origin_mismatch`] when the URL carries userinfo (`user@`, `user:pass@`).
///
/// Ported from `ORIGIN_USERINFO` (`base.py:220`).
pub const ORIGIN_USERINFO: &str = "URL with userinfo";

/// Reason for a URL whose authority cannot be parsed.
///
/// Ported from `ORIGIN_UNPARSEABLE` (`base.py:221`). [`origin_mismatch`] takes an already parsed
/// [`Url`] and therefore never returns it; it is exported so a caller that starts from a raw
/// string can report a [`Url::parse`] failure (a bad port, an unterminated IPv6 literal, a
/// forbidden host character) with the same phrase Python logs.
pub const ORIGIN_UNPARSEABLE: &str = "unparseable authority";

/// Reason for a configured base URL that cannot define an origin.
///
/// Ported from `ORIGIN_BAD_BASE` (`base.py:222`). Like [`ORIGIN_UNPARSEABLE`] it is never returned
/// by [`origin_mismatch`], whose `base` is a parsed [`Url`]; it is exported for a caller that
/// holds the base as a raw string.
pub const ORIGIN_BAD_BASE: &str = "unusable base URL";

/// The outcome of a failed [`origin_mismatch`] comparison.
///
/// Ported from `OriginMismatch` (`base.py:225-237`). Both fields are safe to log: `reason` is one
/// of the fixed `ORIGIN_*` phrases and `target` never carries userinfo, a port, a path or a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginMismatch {
    /// One of the `ORIGIN_*` phrases ([`ORIGIN_FOREIGN_HOST`], [`ORIGIN_SCHEME`],
    /// [`ORIGIN_PORT`] or [`ORIGIN_USERINFO`]).
    pub reason: &'static str,
    /// `scheme://host` of the offending URL, lower-cased, for logging (`<unknown>` stands in for
    /// a missing host). `None` when the URL carried userinfo, so a credential embedded in a URL is
    /// never echoed.
    pub target: Option<String>,
}

/// Decides whether `url` is on the same origin as the configured API `base`.
///
/// This is the one origin rule of the crate, shared by the link validator and the credential
/// gate. Origin means scheme, host and *effective* port, each compared case-insensitively, where
/// the effective port is the explicit one or else the scheme's default (443 for `https`, 80 for
/// `http`), so `https://h:443/` and `https://h/` are the same origin. A URL carrying userinfo is
/// never on the origin, whatever its host: userinfo is how a request is made to look like one
/// host while addressing another.
///
/// Ported from `origin_mismatch` and `_parse_origin` (`base.py:240-308`, with `_DEFAULT_PORTS` at
/// `:213`). Returns `None` when `url` is on the origin; otherwise the first check that failed, in
/// Python's order: userinfo, host, scheme, port. The userinfo of `base` is ignored, as in Python
/// (a base URL is validated when the client is configured, not here).
///
/// # Differences from Python
///
/// Python compares raw text, so it has parse-failure outcomes. A [`Url`] has already been through
/// the WHATWG parser, which changes what is and is not reachable here:
///
/// - **Unreachable.** [`ORIGIN_UNPARSEABLE`] and [`ORIGIN_BAD_BASE`] cannot be returned: a bad
///   port (`:abc`, `:99999`), an unterminated IPv6 literal and forbidden host characters (such as
///   a fullwidth `：`) make [`Url::parse`] fail before a `Url` exists. The scheme and host are
///   already lower-cased, tab, carriage return and line feed are already stripped, an empty port
///   (`host:`) and leading zeros (`:0443`) are normalised, and an explicit default port is
///   dropped by the parser. None of those need a check here.
/// - **Still needs a check on the raw string**, because the parser silently rewrites it into
///   something that looks innocent. A caller that starts from text must inspect that text before
///   parsing it:
///   - a backslash in the authority: `https://base.host\@evil.example/x` parses as host
///     `base.host` with path `/@evil.example/x`, so it is on the origin here although Python
///     reports [`ORIGIN_UNPARSEABLE`];
///   - an empty userinfo (`https://@base.host/`, `https://:@base.host/`) is dropped by the
///     parser, so it is invisible here although Python reports [`ORIGIN_USERINFO`];
///   - non-ASCII characters that IDNA maps to ASCII (`https://ａpi.base.host/`) and
///     percent-escapes (`https://api%2ebase.host/`) are folded into the host, which Python
///     keeps distinct and reports as [`ORIGIN_FOREIGN_HOST`].
///
///   In each case the request still goes to the host the `Url` names, never to the one the text
///   appears to name.
/// - Effective ports use [`Url::port_or_known_default`], which for `http` and `https` is exactly
///   Python's `_DEFAULT_PORTS` table; it also knows the defaults of other schemes, which only
///   matters for a non-HTTP base.
///
/// # Examples
///
/// ```
/// use rusteero::links::{ORIGIN_PORT, origin_mismatch};
/// use url::Url;
///
/// let base = Url::parse("https://api.example.test").unwrap();
/// let same = Url::parse("https://API.example.test:443/2.2/networks").unwrap();
/// assert!(origin_mismatch(&same, &base).is_none());
///
/// let other_port = Url::parse("https://api.example.test:8443/2.2/networks").unwrap();
/// let mismatch = origin_mismatch(&other_port, &base).unwrap();
/// assert_eq!(mismatch.reason, ORIGIN_PORT);
/// assert_eq!(mismatch.target.as_deref(), Some("https://api.example.test"));
/// ```
#[must_use]
pub fn origin_mismatch(url: &Url, base: &Url) -> Option<OriginMismatch> {
    if !url.username().is_empty() || url.password().is_some() {
        return Some(OriginMismatch {
            reason: ORIGIN_USERINFO,
            target: None,
        });
    }

    let host = url.host_str().map(str::to_ascii_lowercase);
    let target = format!(
        "{}://{}",
        url.scheme().to_ascii_lowercase(),
        host.as_deref().unwrap_or("<unknown>")
    );
    let mismatch = |reason| {
        Some(OriginMismatch {
            reason,
            target: Some(target.clone()),
        })
    };

    let same_host = match (host.as_deref(), base.host_str()) {
        (Some(host), Some(base_host)) => host.eq_ignore_ascii_case(base_host),
        _ => false,
    };
    if !same_host {
        return mismatch(ORIGIN_FOREIGN_HOST);
    }
    if !url.scheme().eq_ignore_ascii_case(base.scheme()) {
        return mismatch(ORIGIN_SCHEME);
    }
    if url.port_or_known_default() != base.port_or_known_default() {
        return mismatch(ORIGIN_PORT);
    }
    None
}

/// Logs exactly one fixed `WARNING` before issuing a write whose side effects have not been
/// fully characterised against a live network.
///
/// Ported from `warn_uncharacterised_write` (`_writes.py:37-64`). `operation` must be a short,
/// fixed description of the write — e.g. `"set nightlight for eero"` — never a caller-supplied
/// identifier (network ID, profile, MAC, eero serial, ...), which would otherwise end up in a
/// `WARNING`-level log line.
pub fn warn_uncharacterised_write(operation: &str) {
    tracing::warn!(
        operation,
        "Issuing write ({operation}): its side effects have not been fully characterised \
         against a live network. Read the current state first and skip the write when it \
         already matches -- never retry a failed write in a loop."
    );
}

#[cfg(test)]
mod tests {
    use super::{
        ORIGIN_FOREIGN_HOST, ORIGIN_PORT, ORIGIN_SCHEME, ORIGIN_USERINFO, OriginMismatch,
        child_url, is_version_segment, join_api_path, origin_mismatch, resolve_link, resource_url,
        rewrite_version, self_url, sub_resource_url, validate_identifier,
    };
    use crate::error::Error;
    use crate::routes::ApiVersion;
    use serde_json::json;
    use url::Url;

    /// Deliberately NOT the production host: every helper must resolve against the `host` it is
    /// given (the transport's configured base, a wiremock server in the integration suite), and a
    /// fixture equal to `consts::API_HOST` once hid a branch that read the production constant
    /// instead of `host`.
    fn host() -> Url {
        Url::parse("http://mock.test:1234").unwrap()
    }

    // ===================== validate_identifier / TestIdentifierValidation =====================

    #[test]
    fn valid_identifiers_are_accepted() {
        assert_eq!(validate_identifier("abc123").unwrap(), "abc123");
        assert_eq!(validate_identifier("p_abc-1.2:3").unwrap(), "p_abc-1.2:3");
    }

    #[test]
    fn empty_identifier_is_rejected() {
        let err = validate_identifier("").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    }

    #[test]
    fn identifier_starting_with_punctuation_is_rejected() {
        assert!(validate_identifier("-abc").is_err());
        assert!(validate_identifier(".abc").is_err());
    }

    #[test]
    fn identifier_with_dot_dot_is_rejected_even_if_it_matches_the_character_class() {
        assert!(validate_identifier("a..b").is_err());
    }

    #[test]
    fn identifier_with_slash_or_whitespace_is_rejected() {
        assert!(validate_identifier("a/b").is_err());
        assert!(validate_identifier("a b").is_err());
    }

    // ===================== join_api_path / TestJoinApiPath =====================

    #[test]
    fn join_api_path_joins_host_and_path() {
        let url = join_api_path(&host(), "/2.2/networks/1/eeros").unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/1/eeros");
    }

    #[test]
    fn join_api_path_rejects_empty_path() {
        let err = join_api_path(&host(), "").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "path"));
    }

    // ===================== resource_url / TestResourceUrl / TestResourceUrlSuffix =====================

    #[test]
    fn resource_url_bare_id_substitutes_into_template() {
        let url = resource_url(&host(), "100", "networks/{id}", ApiVersion::V2_2).unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/100");
    }

    #[test]
    fn resource_url_with_suffix_appends_after_the_id() {
        let url = resource_url(&host(), "100", "networks/{id}/settings", ApiVersion::V2_2).unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/settings"
        );
    }

    #[test]
    fn resource_url_absolute_url_on_host_is_validated_and_suffix_appended() {
        let url = resource_url(
            &host(),
            "http://mock.test:1234/2.3/networks/100",
            "networks/{id}/settings",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.3/networks/100/settings"
        );
    }

    #[test]
    fn resource_url_absolute_url_off_host_is_rejected() {
        let err = resource_url(
            &host(),
            "https://evil.example/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "url"));
    }

    #[test]
    fn resource_url_userinfo_trick_is_rejected() {
        let err = resource_url(
            &host(),
            "http://mock.test:1234@evil.example/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    #[test]
    fn resource_url_suffix_trick_is_rejected() {
        let err = resource_url(
            &host(),
            "http://mock.test:1234.evil.example/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    // ===================== security finding 3: query/fragment rejection =====================

    #[test]
    fn resource_url_absolute_url_with_query_is_rejected() {
        let err = resource_url(
            &host(),
            "http://mock.test:1234/2.2/networks/100?evil=1",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "url"));
    }

    #[test]
    fn resource_url_absolute_url_with_fragment_is_rejected() {
        let err = resource_url(
            &host(),
            "http://mock.test:1234/2.2/networks/100#frag",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "url"));
    }

    #[test]
    fn resource_url_path_prefixed_id_with_query_is_rejected() {
        let err = resource_url(
            &host(),
            "/2.2/networks/100?evil=1",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "link"));
    }

    #[test]
    fn resource_url_path_prefixed_id_with_fragment_is_rejected() {
        let err = resource_url(
            &host(),
            "/2.2/networks/100#frag",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "link"));
    }

    #[test]
    fn validate_link_path_via_resolve_link_rejects_a_query_string() {
        let parent = json!({"resources": {"eeros": "/2.2/networks/100/eeros?evil=1"}});
        let err = resolve_link(&host(), &parent, "eeros").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "link"));
    }

    #[test]
    fn resource_url_path_prefixed_id_is_joined_onto_host() {
        let url = resource_url(
            &host(),
            "/2.3/networks/100",
            "networks/{id}/settings",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.3/networks/100/settings"
        );
    }

    #[test]
    fn resource_url_rejects_template_without_placeholder() {
        let err = resource_url(&host(), "100", "networks", ApiVersion::V2_2).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "template"));
    }

    #[test]
    fn resource_url_rejects_empty_id() {
        let err = resource_url(&host(), "", "networks/{id}", ApiVersion::V2_2).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
    }

    // ===================== TestVersionConstants =====================

    #[test]
    fn version_constants_select_the_right_base_host() {
        let url = resource_url(&host(), "100", "networks/{id}", ApiVersion::V2_3).unwrap();
        assert!(url.as_str().starts_with("http://mock.test:1234/2.3/"));
    }

    // ===================== child_url =====================

    #[test]
    fn child_url_appends_validated_child_id() {
        let base = Url::parse("http://mock.test:1234/2.2/networks/100/blacklist").unwrap();
        let url = child_url(&base, "aa:bb:cc").unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/blacklist/aa:bb:cc"
        );
    }

    // ===================== resolve_link / self_url / TestResolveLink / TestSelfUrl =====================

    #[test]
    fn resolve_link_reads_named_resource_from_full_envelope() {
        let parent = json!({
            "meta": {"code": 200},
            "data": {"resources": {"eeros": "/2.2/networks/100/eeros"}},
        });
        let url = resolve_link(&host(), &parent, "eeros").unwrap().unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/100/eeros");
    }

    #[test]
    fn resolve_link_accepts_unwrapped_data_object() {
        let data = json!({"resources": {"eeros": "/2.2/networks/100/eeros"}});
        let url = resolve_link(&host(), &data, "eeros").unwrap().unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/100/eeros");
    }

    #[test]
    fn resolve_link_missing_link_is_ok_none() {
        let data = json!({"resources": {}});
        assert!(resolve_link(&host(), &data, "eeros").unwrap().is_none());
        let data = json!({});
        assert!(resolve_link(&host(), &data, "eeros").unwrap().is_none());
    }

    #[test]
    fn resolve_link_malformed_link_value_errors() {
        let data = json!({"resources": {"eeros": "https://evil.example/x"}});
        let err = resolve_link(&host(), &data, "eeros").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "link"));
    }

    #[test]
    fn self_url_reads_url_field() {
        let data = json!({"url": "/2.2/networks/100"});
        let url = self_url(&host(), &data).unwrap().unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/100");
    }

    #[test]
    fn self_url_missing_is_ok_none() {
        let data = json!({});
        assert!(self_url(&host(), &data).unwrap().is_none());
    }

    // ===================== sub_resource_url / TestSubResourceUrl =====================

    #[test]
    fn sub_resource_url_prefers_the_parents_published_link() {
        let parent = json!({"resources": {"settings": "/2.4/networks/100/settings"}});
        let url = sub_resource_url(
            &host(),
            "100",
            "networks/{id}/settings",
            "settings",
            Some(&parent),
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.4/networks/100/settings"
        );
    }

    #[test]
    fn sub_resource_url_falls_back_to_resource_url_when_no_parent() {
        let url = sub_resource_url(
            &host(),
            "100",
            "networks/{id}/settings",
            "settings",
            None,
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/settings"
        );
    }

    #[test]
    fn sub_resource_url_falls_back_when_parent_has_no_such_link() {
        let parent = json!({"resources": {}});
        let url = sub_resource_url(
            &host(),
            "100",
            "networks/{id}/settings",
            "settings",
            Some(&parent),
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/settings"
        );
    }

    // ===================== rewrite_version / TestRewriteVersion =====================

    /// Parses `raw`, pins it to `version` and returns the result as a string.
    fn pinned(raw: &str, version: ApiVersion) -> String {
        rewrite_version(&Url::parse(raw).unwrap(), version).to_string()
    }

    #[test]
    fn rewrite_version_rewrites_23_to_22() {
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.3/networks/network-id-placeholder/support",
                ApiVersion::V2_2
            ),
            "http://mock.test:1234/2.2/networks/network-id-placeholder/support"
        );
    }

    #[test]
    fn rewrite_version_rewrites_22_to_23() {
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.2/networks/n1/devices/x",
                ApiVersion::V2_3
            ),
            "http://mock.test:1234/2.3/networks/n1/devices/x"
        );
    }

    #[test]
    fn rewrite_version_is_a_noop_when_already_on_the_target_version() {
        let raw = "http://mock.test:1234/2.2/networks/network-id-placeholder/routing";
        assert_eq!(pinned(raw, ApiVersion::V2_2), raw);
        let raw = "http://mock.test:1234/2.3/networks/n1/devices/x";
        assert_eq!(pinned(raw, ApiVersion::V2_3), raw);
    }

    #[test]
    fn rewrite_version_leaves_a_url_without_a_version_segment_unchanged() {
        for raw in [
            "http://mock.test:1234/login",
            "http://mock.test:1234/networks/n1",
            "http://mock.test:1234/",
            "http://mock.test:1234",
        ] {
            assert_eq!(
                pinned(raw, ApiVersion::V2_2),
                Url::parse(raw).unwrap().as_str()
            );
        }
    }

    #[test]
    fn rewrite_version_needs_a_slash_after_the_version_segment() {
        // Python's `path.split("/", 2)` must yield three parts: `/2.3` yields two, `/2.3/` three.
        assert_eq!(
            pinned("http://mock.test:1234/2.3", ApiVersion::V2_2),
            "http://mock.test:1234/2.3"
        );
        assert_eq!(
            pinned("http://mock.test:1234/2.3/", ApiVersion::V2_2),
            "http://mock.test:1234/2.2/"
        );
    }

    #[test]
    fn rewrite_version_only_matches_digits_dot_digits() {
        for raw in [
            "http://mock.test:1234/v2/x/y",
            "http://mock.test:1234/2/x",
            "http://mock.test:1234/2.2.2/x",
            "http://mock.test:1234/.2/x",
            "http://mock.test:1234/2./x",
            "http://mock.test:1234/2.x/x",
            "http://mock.test:1234/%32.%33/x",
            // Arabic-Indic digits are numeric but not `[0-9]`; they are percent-encoded by the
            // parser, so they can never reach the digit test as digits.
            "http://mock.test:1234/\u{662}.\u{663}/x",
        ] {
            assert_eq!(
                pinned(raw, ApiVersion::V2_2),
                Url::parse(raw).unwrap().as_str(),
                "{raw}"
            );
        }
    }

    #[test]
    fn rewrite_version_accepts_multi_digit_versions() {
        assert_eq!(
            pinned("http://mock.test:1234/2.10/x", ApiVersion::V2_2),
            "http://mock.test:1234/2.2/x"
        );
        assert_eq!(
            pinned("http://mock.test:1234/10.0/x", ApiVersion::V2_3),
            "http://mock.test:1234/2.3/x"
        );
    }

    #[test]
    fn rewrite_version_scheme_case_does_not_defeat_the_rewrite() {
        for scheme in ["HTTPS", "Https", "hTTps"] {
            assert_eq!(
                pinned(
                    &format!(
                        "{scheme}://mock.test:1234/2.3/networks/network-id-placeholder/support"
                    ),
                    ApiVersion::V2_2
                ),
                "https://mock.test:1234/2.2/networks/network-id-placeholder/support",
                "{scheme}"
            );
        }
    }

    #[test]
    fn rewrite_version_control_character_in_path_does_not_defeat_the_rewrite() {
        // Tab, CR and LF are dropped by the URL parser and form feed is percent-encoded; in every
        // case the version segment is still found and replaced.
        for control in ["\n", "\t", "\r", "\u{c}"] {
            let raw = format!(
                "http://mock.test:1234/2.3/networks/network-id{control}placeholder/support"
            );
            let rewritten = pinned(&raw, ApiVersion::V2_2);
            assert!(
                rewritten.starts_with("http://mock.test:1234/2.2/networks/network-id"),
                "{rewritten}"
            );
            assert!(rewritten.ends_with("placeholder/support"), "{rewritten}");
            assert!(!rewritten.contains("/2.3/"), "{rewritten}");
        }
    }

    #[test]
    fn rewrite_version_only_the_first_path_segment_is_replaced() {
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.3/networks/network-id-placeholder/2.3/support",
                ApiVersion::V2_2
            ),
            "http://mock.test:1234/2.2/networks/network-id-placeholder/2.3/support"
        );
        assert_eq!(
            pinned("http://mock.test:1234/2.2/networks/2.2/x", ApiVersion::V2_3),
            "http://mock.test:1234/2.3/networks/2.2/x"
        );
    }

    #[test]
    fn rewrite_version_preserves_query_and_fragment() {
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.3/networks/network-id-placeholder/support?a=1#frag",
                ApiVersion::V2_2
            ),
            "http://mock.test:1234/2.2/networks/network-id-placeholder/support?a=1#frag"
        );
    }

    #[test]
    fn rewrite_version_preserves_percent_escapes_without_double_encoding() {
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.3/networks/a%20b/support",
                ApiVersion::V2_2
            ),
            "http://mock.test:1234/2.2/networks/a%20b/support"
        );
        assert_eq!(
            pinned(
                "http://mock.test:1234/2.3/networks/a%2Fb/x",
                ApiVersion::V2_2
            ),
            "http://mock.test:1234/2.2/networks/a%2Fb/x"
        );
    }

    #[test]
    fn rewrite_version_preserves_empty_segments_and_trailing_slash() {
        assert_eq!(
            pinned("http://mock.test:1234/2.3//x/", ApiVersion::V2_2),
            "http://mock.test:1234/2.2//x/"
        );
    }

    #[test]
    fn rewrite_version_preserves_scheme_host_and_port() {
        assert_eq!(
            pinned("https://mock.test:8443/2.3/x/y", ApiVersion::V2_2),
            "https://mock.test:8443/2.2/x/y"
        );
        assert_eq!(
            pinned("https://mock.test/2.3/x/y", ApiVersion::V2_2),
            "https://mock.test/2.2/x/y"
        );
        assert_eq!(
            pinned("http://[::1]:8080/2.3/x/y", ApiVersion::V2_2),
            "http://[::1]:8080/2.2/x/y"
        );
    }

    #[test]
    fn rewrite_version_leaves_non_http_and_hostless_urls_unchanged() {
        for raw in [
            "ftp://mock.test/2.3/x/y",
            "ws://mock.test/2.3/x/y",
            "file:///2.3/x/y",
            "mailto:2.3/x/y",
            "data:text/plain,2.3/x/y",
        ] {
            assert_eq!(
                pinned(raw, ApiVersion::V2_2),
                Url::parse(raw).unwrap().as_str(),
                "{raw}"
            );
        }
    }

    #[test]
    fn rewrite_version_does_not_modify_its_input() {
        let url = Url::parse("http://mock.test:1234/2.3/x/y").unwrap();
        let _ = rewrite_version(&url, ApiVersion::V2_2);
        assert_eq!(url.as_str(), "http://mock.test:1234/2.3/x/y");
    }

    #[test]
    fn rewrite_version_has_no_counterpart_for_a_relative_url() {
        // Python's "relative-path" case: a `Url` is always absolute, so the input cannot exist.
        assert!(Url::parse("/2.3/networks/network-id-placeholder/support").is_err());
    }

    #[test]
    fn is_version_segment_matches_ascii_digits_dot_digits_only() {
        for good in ["2.2", "2.3", "2.10", "10.0", "0.0"] {
            assert!(is_version_segment(good), "{good}");
        }
        for bad in [
            "",
            "2",
            "2.",
            ".2",
            ".",
            "2.2.2",
            "v2.2",
            "2.2a",
            "2 .2",
            "\u{662}.\u{663}",
        ] {
            assert!(!is_version_segment(bad), "{bad:?}");
        }
    }

    // ===================== origin_mismatch / TestOriginMismatch =====================

    /// The configured origin the cases below are compared against (Python's
    /// `TestOriginMismatch.BASE`); not the production host.
    fn origin_base() -> Url {
        Url::parse("https://api.example.com").unwrap()
    }

    fn mismatch_of(raw: &str) -> Option<OriginMismatch> {
        origin_mismatch(&Url::parse(raw).unwrap(), &origin_base())
    }

    #[test]
    fn origin_mismatch_same_origin() {
        // Case and an explicit or empty default port do not change the origin.
        for raw in [
            "https://api.example.com/x",
            "https://api.example.com:443/x",
            "https://api.example.com:/x",
            "https://api.example.com:0443/x",
            "HTTPS://API.EXAMPLE.COM/x",
            "https://api.example.com/x?q=1#f",
        ] {
            assert_eq!(mismatch_of(raw), None, "{raw}");
        }
    }

    #[test]
    fn origin_mismatch_reasons() {
        let cases = [
            ("https://other.example.com/x", ORIGIN_FOREIGN_HOST),
            (
                "https://api.example.com.evil.example/x",
                ORIGIN_FOREIGN_HOST,
            ),
            ("https://api.example.com./x", ORIGIN_FOREIGN_HOST),
            ("https://api.example.com@evil.example/x", ORIGIN_USERINFO),
            ("https://user:pass@api.example.com/x", ORIGIN_USERINFO),
            ("https://user@api.example.com/x", ORIGIN_USERINFO),
            ("https://:pass@api.example.com/x", ORIGIN_USERINFO),
            (
                "https://api.example.com:443@evil.example/x",
                ORIGIN_USERINFO,
            ),
            ("http://api.example.com/x", ORIGIN_SCHEME),
            ("ftp://api.example.com/x", ORIGIN_SCHEME),
            ("https://api.example.com:8443/x", ORIGIN_PORT),
            ("https://api.example.com:80/x", ORIGIN_PORT),
            ("https://api.example.com:0/x", ORIGIN_PORT),
        ];
        for (raw, reason) in cases {
            let result = mismatch_of(raw).unwrap_or_else(|| panic!("{raw} should not match"));
            assert_eq!(result.reason, reason, "{raw}");
        }
    }

    #[test]
    fn origin_mismatch_reports_the_first_failing_check_in_order() {
        // Userinfo beats host, host beats scheme, scheme beats port.
        assert_eq!(
            mismatch_of("http://user@evil.example:8443/x")
                .unwrap()
                .reason,
            ORIGIN_USERINFO
        );
        assert_eq!(
            mismatch_of("http://evil.example:8443/x").unwrap().reason,
            ORIGIN_FOREIGN_HOST
        );
        assert_eq!(
            mismatch_of("http://api.example.com:8443/x").unwrap().reason,
            ORIGIN_SCHEME
        );
    }

    #[test]
    fn origin_mismatch_default_port_is_per_scheme() {
        // Port 80 is the default for http only; 443 is not implied there.
        let base = Url::parse("http://api.example.com").unwrap();
        let on_80 = Url::parse("http://api.example.com:80/x").unwrap();
        let on_443 = Url::parse("http://api.example.com:443/x").unwrap();
        assert_eq!(origin_mismatch(&on_80, &base), None);
        assert_eq!(origin_mismatch(&on_443, &base).unwrap().reason, ORIGIN_PORT);
    }

    #[test]
    fn origin_mismatch_base_with_a_non_default_port() {
        let base = Url::parse("https://api.example.com:8443").unwrap();
        let same = Url::parse("https://api.example.com:8443/x").unwrap();
        let default_port = Url::parse("https://api.example.com/x").unwrap();
        assert_eq!(origin_mismatch(&same, &base), None);
        assert_eq!(
            origin_mismatch(&default_port, &base).unwrap().reason,
            ORIGIN_PORT
        );
        // An explicit default port on the base is the same as none.
        let explicit = Url::parse("https://api.example.com:443").unwrap();
        assert_eq!(origin_mismatch(&default_port, &explicit), None);
    }

    #[test]
    fn origin_mismatch_base_userinfo_and_path_are_ignored() {
        let base = Url::parse("https://user:pw@api.example.com/2.2/ignored?q=1").unwrap();
        let url = Url::parse("https://api.example.com/x").unwrap();
        assert_eq!(origin_mismatch(&url, &base), None);
    }

    #[test]
    fn origin_mismatch_target_is_scheme_and_host_only() {
        assert_eq!(
            mismatch_of("http://api.example.com:81/x?secret=1#frag")
                .unwrap()
                .target
                .as_deref(),
            Some("http://api.example.com")
        );
        assert_eq!(
            mismatch_of("https://Other.Example.COM:8443/a/b?token=abc")
                .unwrap()
                .target
                .as_deref(),
            Some("https://other.example.com")
        );
        // Userinfo is never echoed: no target at all.
        assert_eq!(
            mismatch_of("https://user:pass@api.example.com/x")
                .unwrap()
                .target,
            None
        );
    }

    #[test]
    fn origin_mismatch_target_never_leaks_the_query_or_path() {
        for raw in [
            "https://other.example.com/a/b?token=abc#frag",
            "http://api.example.com/a/b?token=abc#frag",
            "https://api.example.com:8443/a/b?token=abc#frag",
        ] {
            let target = mismatch_of(raw).unwrap().target.unwrap();
            assert!(
                !target.contains("token") && !target.contains("/a/b"),
                "{target}"
            );
            assert!(!target.contains('?') && !target.contains('#'), "{target}");
            assert!(!target.contains("8443"), "{target}");
        }
    }

    #[test]
    fn origin_mismatch_hostless_url_is_a_foreign_host_with_an_unknown_target() {
        // Python's `("/relative/path", "foreign host")` and `("", "foreign host")` cannot be
        // parsed into a `Url`; the closest reachable inputs are hostless absolute URLs.
        for (raw, target) in [
            ("file:///etc/passwd", "file://<unknown>"),
            ("mailto:someone@example.com", "mailto://<unknown>"),
            ("data:text/plain,hello", "data://<unknown>"),
        ] {
            let result = mismatch_of(raw).unwrap();
            assert_eq!(result.reason, ORIGIN_FOREIGN_HOST, "{raw}");
            assert_eq!(result.target.as_deref(), Some(target), "{raw}");
        }
    }

    #[test]
    fn origin_mismatch_hostless_base_matches_nothing() {
        let base = Url::parse("data:text/plain,hello").unwrap();
        let url = Url::parse("https://api.example.com/x").unwrap();
        assert_eq!(
            origin_mismatch(&url, &base).unwrap().reason,
            ORIGIN_FOREIGN_HOST
        );
        assert_eq!(
            origin_mismatch(&base, &base).unwrap().reason,
            ORIGIN_FOREIGN_HOST
        );
    }

    #[test]
    fn origin_mismatch_ipv6_literals_compare_by_value() {
        let base = Url::parse("http://[::1]:8080").unwrap();
        let same = Url::parse("http://[0:0:0:0:0:0:0:1]:8080/x").unwrap();
        let other_port = Url::parse("http://[::1]:9090/x").unwrap();
        let other_host = Url::parse("http://[::2]:8080/x").unwrap();
        assert_eq!(origin_mismatch(&same, &base), None);
        assert_eq!(
            origin_mismatch(&other_port, &base).unwrap().reason,
            ORIGIN_PORT
        );
        assert_eq!(
            origin_mismatch(&other_host, &base).unwrap().reason,
            ORIGIN_FOREIGN_HOST
        );
    }

    #[test]
    fn origin_mismatch_parser_normalisations_agree_with_the_request_destination() {
        // Tab, CR and LF are stripped by the parser (as by the HTTP client), so these name the
        // configured origin and are sent to it.
        for raw in [
            "https://api.example.\tcom/x",
            "https://api.example.\rcom/x",
            "https://api.example.\ncom/x",
            "https://api.example.com:4\t43/x",
            "https://api.example.com\t:443/x",
            "https://api.example.com#@evil.example/x",
            "https://api.example.com?@evil.example/x",
        ] {
            assert_eq!(mismatch_of(raw), None, "{raw}");
        }
        // ...and a stripped control character cannot hide userinfo either.
        assert_eq!(
            mismatch_of("https://api.example.com\n@evil.example/x")
                .unwrap()
                .reason,
            ORIGIN_USERINFO
        );
    }

    #[test]
    fn origin_mismatch_unparseable_authorities_never_become_a_url() {
        // Python reports these as "unparseable authority". With a `Url` input they are rejected
        // earlier, by `Url::parse`; a caller holding the raw text maps that failure to
        // `ORIGIN_UNPARSEABLE`.
        for raw in [
            "https://api.example.com:99999/x",
            "https://api.example.com:abc/x",
            "https://api.example.com\u{ff1a}99/x",
            "https://[::1/x",
            "https://api.example.com%40evil.example/x",
            "/relative/path",
            "",
        ] {
            assert!(Url::parse(raw).is_err(), "{raw:?} unexpectedly parsed");
        }
    }

    #[test]
    fn origin_mismatch_fullwidth_solidus_before_an_at_sign_is_userinfo() {
        // Python: "unparseable authority" (NFKC maps `／` to `/`). The URL parser keeps `／` as an
        // ordinary character, so the text before `@` is userinfo and the real host is
        // `evil.example`; the URL is rejected either way.
        assert_eq!(
            mismatch_of("https://api.example.com\u{ff0f}@evil.example/x")
                .unwrap()
                .reason,
            ORIGIN_USERINFO
        );
    }

    #[test]
    fn origin_mismatch_raw_text_forms_the_parser_rewrites_need_a_text_check() {
        // These are rejected by Python but are rewritten by the parser into something that does
        // name the configured origin (or a different one), so `origin_mismatch` cannot see the
        // original form. Pinned so a later change to the parser's behaviour is noticed.
        //
        // A backslash is a path separator for http(s): the host is `api.example.com`.
        assert_eq!(
            mismatch_of("https://api.example.com\\@evil.example/x"),
            None
        );
        // ...and here it ends the authority at `evil.example`.
        assert_eq!(
            mismatch_of("https://evil.example\\@api.example.com/x")
                .unwrap()
                .reason,
            ORIGIN_FOREIGN_HOST
        );
        // An empty userinfo is dropped.
        assert_eq!(mismatch_of("https://@api.example.com/x"), None);
        assert_eq!(mismatch_of("https://:@api.example.com/x"), None);
        // IDNA folds a fullwidth letter into ASCII, and a percent-escape in the host is decoded.
        assert_eq!(mismatch_of("https://\u{ff41}pi.example.com/x"), None);
        assert_eq!(mismatch_of("https://api%2eexample.com/x"), None);
    }

    #[test]
    fn origin_mismatch_reason_phrases_are_the_fixed_python_texts() {
        assert_eq!(ORIGIN_FOREIGN_HOST, "foreign host");
        assert_eq!(ORIGIN_SCHEME, "non-matching scheme");
        assert_eq!(ORIGIN_PORT, "non-matching port");
        assert_eq!(ORIGIN_USERINFO, "URL with userinfo");
        assert_eq!(super::ORIGIN_UNPARSEABLE, "unparseable authority");
        assert_eq!(super::ORIGIN_BAD_BASE, "unusable base URL");
    }

    // ===================== TestReadOnlyMappingParents =====================

    #[test]
    fn resolve_link_never_mutates_the_parent_value() {
        let parent = json!({"resources": {"eeros": "/2.2/networks/100/eeros"}});
        let before = parent.clone();
        let _ = resolve_link(&host(), &parent, "eeros");
        assert_eq!(parent, before);
    }
}
