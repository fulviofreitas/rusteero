//! Resource-link resolution for Eero API response envelopes.
//!
//! Ported from `eero-api`'s `src/eero/api/links.py` at `v8.4.1`. The API returns
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
/// Ported from `_validate_identifier`/`validate_identifier` (`links.py:68-105`): the identifier
/// regex `^[A-Za-z0-9][A-Za-z0-9._:-]*\Z`, plus a literal `".."` rejection (so `"a..b"` is
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

/// Rejects a path or URL that could address a different resource than the one it names.
///
/// Ported from `_require_plain_path` (`links.py:108-138`), the one rule for the shape of any path
/// or URL about to be placed on a request line, whether the API published it or a caller supplied
/// it. Checked in Python's order, on the raw string:
///
/// 1. whitespace or a control character anywhere (Python's `[\s\x00-\x1f\x7f-\x9f]`, which is
///    exactly [`char::is_whitespace`] or [`char::is_control`]);
/// 2. a `?` or `#`;
/// 3. a whole `.` or `..` segment (split on `/`), or a percent-encoded dot (`%2e`, any case)
///    anywhere. Dots inside a segment (`a..b`, `v1.2`) are fine.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` and one of `"must not contain whitespace or control
/// characters"`, `"must not carry a query or fragment"` or `"must not contain dot segments"`.
fn require_plain_path<'a>(value: &'a str, field: &str) -> Result<&'a str, Error> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::validation(
            field,
            "must not contain whitespace or control characters",
        ));
    }
    if value.contains(['?', '#']) {
        return Err(Error::validation(
            field,
            "must not carry a query or fragment",
        ));
    }
    let encoded_dot = value
        .as_bytes()
        .windows(3)
        .any(|w| w[0] == b'%' && w[1] == b'2' && w[2].eq_ignore_ascii_case(&b'e'));
    if encoded_dot
        || value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(Error::validation(field, "must not contain dot segments"));
    }
    Ok(value)
}

/// Rejects a backslash anywhere in a path or URL.
///
/// Not part of Python's rule, and therefore stricter than it: the URL parser used here turns a
/// backslash into a slash for `http(s)`, so `/2.2/networks/n\..\account` would be resolved to a
/// different path than the one that was checked, while other parsers keep it as a literal
/// character. Neither reading is something an API path ever needs.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` and `"must not contain a backslash"`.
fn require_no_backslash<'a>(value: &'a str, field: &str) -> Result<&'a str, Error> {
    if value.contains('\\') {
        return Err(Error::validation(field, "must not contain a backslash"));
    }
    Ok(value)
}

/// Validates a host-relative path before joining it onto the API host.
///
/// Ported from `_validate_link_path` (`links.py:141-163`). `field` names the parameter reported
/// in the error: `"link"` for a value read from an envelope, `"id_or_url"` for a value a caller
/// supplied. The path is deliberately **not** confined to a resource family or a version: a
/// published link may name any resource, on whatever version the API serves it.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` if `link` is empty, contains `"://"`, or starts with
/// `"//"` (none of which an API-published path ever does), or fails the plain-path rule
/// (whitespace or a control character, a query or fragment, a dot segment) or contains a
/// backslash. Every caller joins `link` verbatim onto the API host via [`join_api_path`], so a
/// query, fragment or dot segment slipped through here would ride along into the resolved request
/// URL unexamined.
fn validate_link_path<'a>(link: &'a str, field: &str) -> Result<&'a str, Error> {
    if link.is_empty() || link.contains("://") || link.starts_with("//") {
        return Err(Error::validation(field, "must be a host-relative API path"));
    }
    require_plain_path(link, field)?;
    require_no_backslash(link, field)
}

/// Returns the `data` object of `parent`, without copying it, when `parent` looks like a full
/// response envelope (has both a `meta` key and an object `data` key); otherwise returns `parent`
/// unchanged.
///
/// Ported from `_as_data` (`links.py:166-182`). `pub(crate)` so other modules that need the
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
/// Ported from `join_api_path` (`links.py:185-201`).
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

/// Returns the length of the `http://` or `https://` prefix of `value`, compared ASCII
/// case-insensitively, or `None` if `value` starts with neither.
fn http_scheme_prefix_len(value: &str) -> Option<usize> {
    ["http://", "https://"].into_iter().find_map(|prefix| {
        value
            .get(..prefix.len())
            .filter(|head| head.eq_ignore_ascii_case(prefix))
            .map(|_| prefix.len())
    })
}

/// The raw authority of an `http(s)` URL: the text after the scheme's `//` up to the first `/`,
/// `?` or `#`, exactly Python's `urlsplit(...).netloc`. Empty for anything else.
fn raw_authority(url: &str) -> &str {
    let Some(prefix) = http_scheme_prefix_len(url) else {
        return "";
    };
    url[prefix..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
}

/// The raw path of an `http(s)` URL (everything from the first `/` after the authority, empty if
/// there is none), or `value` itself when it is not an `http(s)` URL.
fn raw_path(value: &str) -> &str {
    let Some(prefix) = http_scheme_prefix_len(value) else {
        return value;
    };
    let rest = &value[prefix..];
    rest.find('/').map_or("", |start| &rest[start..])
}

/// The hostname inside a raw authority: no userinfo (callers have rejected it), no port, the
/// brackets of an IPv6 literal kept.
fn raw_hostname(authority: &str) -> &str {
    if authority.starts_with('[') {
        authority
            .find(']')
            .map_or(authority, |end| &authority[..=end])
    } else {
        authority.split(':').next().unwrap_or_default()
    }
}

/// Validates that an absolute URL is on `host`'s origin.
///
/// Ported from `_validate_absolute_url` (`links.py:204-236`). The URL must pass the plain-path rule
/// first (so `https://evil.example/x?y=1` reports the query, not the origin), and is then
/// compared with `host` by [`origin_mismatch`]: scheme and host case-insensitively, the *effective*
/// port (an explicit default port is the same origin as none), and no userinfo at all, even when
/// the host after the `@` is the right one. Suffix tricks such as
/// `https://api-user.e2ro.com.evil.example/...` and userinfo tricks such as
/// `https://api-user.e2ro.com@evil.example/...` fail the same comparison. Every origin failure,
/// including an authority that cannot be parsed (a bad port, an unterminated IPv6 literal, a
/// fullwidth `：`), reports the one fixed message below, which never echoes the value.
///
/// # Differences from Python
///
/// Python compares raw text; this function compares a parsed [`Url`], whose parser rewrites some
/// spellings into the configured origin (`https://a@host` to userinfo-free `https://host`, a
/// backslash into a slash, a fullwidth letter into ASCII, `%6d` into `m`). Python rejects all of
/// those, so the raw authority is inspected before parsing: a `@`, a backslash, a `%` or any
/// non-ASCII character in it is an origin failure. The `%` and `@` and backslash rules are exact;
/// the non-ASCII rule is stricter than Python only for a Unicode-form host, which could never
/// equal an ASCII or punycode configured host in Python either. The parsed host must also be
/// spelled as written (`http://127.1/` is not `127.0.0.1`), as in Python. A backslash anywhere
/// else in the URL is rejected by the stricter-than-Python rule described in the module docs.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "url"`: for the plain-path rule's messages, or
/// `"must be an absolute <scheme> URL on <host> with no userinfo and no other port"` for any
/// origin failure.
fn validate_absolute_url(url: &str, host: &Url) -> Result<Url, Error> {
    require_plain_path(url, "url")?;
    let off_origin = || {
        Error::validation(
            "url",
            format!(
                "must be an absolute {} URL on {} with no userinfo and no other port",
                host.scheme(),
                host.host_str().unwrap_or_default()
            ),
        )
    };

    let authority = raw_authority(url);
    if !authority.is_ascii() || authority.contains(['@', '\\', '%']) {
        return Err(off_origin());
    }
    let parsed = Url::parse(url).map_err(|_| off_origin())?;
    let spelled_as_written = parsed
        .host_str()
        .is_some_and(|parsed_host| parsed_host == raw_hostname(authority).to_ascii_lowercase());
    if !spelled_as_written || origin_mismatch(&parsed, host).is_some() {
        return Err(off_origin());
    }
    require_no_backslash(url, "url")?;
    Ok(parsed)
}

/// Resolves a named resource link from a parent envelope to an absolute [`Url`].
///
/// Reads `parent["resources"][name]` (or `parent["data"]["resources"][name]` when `parent` is a
/// full envelope) and joins it onto `host`. Ported from `resolve_link` (`links.py:239-265`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "link"` if the link value read from the envelope is
/// malformed: it contains `"://"`, starts with `"//"`, or fails the plain-path rule (whitespace or
/// a control character, a query or fragment, a dot segment) or contains a backslash. The link is
/// not confined to a resource family or a version. Never errors for a *missing* link — that
/// returns `Ok(None)`.
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
    join_api_path(host, validate_link_path(link, "link")?).map(Some)
}

/// Resolves a parent envelope's own `url` link to an absolute [`Url`].
///
/// Ported from `self_url` (`links.py:268-283`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "link"` if the `url` value read from the envelope is
/// malformed (as for [`resolve_link`]). Never errors when no `url` field is present — that returns
/// `Ok(None)`.
pub fn self_url(host: &Url, parent: &Value) -> Result<Option<Url>, Error> {
    let data = as_data(parent);
    let Some(url) = data.get("url").and_then(Value::as_str) else {
        return Ok(None);
    };
    if url.is_empty() {
        return Ok(None);
    }
    join_api_path(host, validate_link_path(url, "link")?).map(Some)
}

/// One piece of a family pattern: literal text, or one `{id}` placeholder.
enum FamilyPiece<'a> {
    Literal(&'a str),
    Id,
}

/// Whether `c` may appear in an identifier segment after its first character:
/// `[A-Za-z0-9._:-]`.
fn is_segment_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-')
}

/// Matches `rest` against `pieces`, then an optional single `/`, then the end, collecting what
/// each `{id}` captured. An id is one segment: an ASCII alphanumeric followed by
/// [`is_segment_char`]s, as long as the rest of the pattern still matches (greedy, with
/// backtracking, like the regular expression it stands in for).
fn match_family<'p>(rest: &'p str, pieces: &[FamilyPiece<'_>], ids: &mut Vec<&'p str>) -> bool {
    let Some((piece, tail)) = pieces.split_first() else {
        return rest.is_empty() || rest == "/";
    };
    match piece {
        FamilyPiece::Literal(text) => rest
            .strip_prefix(text)
            .is_some_and(|after| match_family(after, tail, ids)),
        FamilyPiece::Id => {
            if !rest
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            {
                return false;
            }
            // Every character of the run is ASCII, so each `end` is a char boundary.
            let longest = rest.find(|c| !is_segment_char(c)).unwrap_or(rest.len());
            for end in (1..=longest).rev() {
                ids.push(&rest[..end]);
                if match_family(&rest[end..], tail, ids) {
                    return true;
                }
                ids.pop();
            }
            false
        }
    }
}

/// Requires a URL's path to name exactly one resource of an expected family, and returns the
/// identifiers it carries.
///
/// The path must be `/<version>/<family><suffix>` with an optional single trailing `/`, where the
/// version is two runs of ASCII digits around a `.` (`2.2`), every `{id}` in `family` stands for
/// one path segment that starts with an ASCII alphanumeric and continues with `[A-Za-z0-9._:-]`,
/// and `suffix` is literal text. Nothing else is accepted: no other resource family, no extra or
/// missing components, no query or fragment, no dot segment, no percent-escape, no whitespace or
/// control character, no backslash. The check runs on the raw `url` text, never on a parsed
/// [`Url`], whose parser would first normalise away exactly what this is looking for.
///
/// `url` is an absolute `http(s)` URL (scheme compared case-insensitively) or a host-relative
/// path. The host of an absolute URL is **not** examined; validate it first, as [`resource_url`]
/// does. The helper does not trim trailing slashes: exactly one is tolerated, two are not, and a
/// caller that wants to tolerate more strips them before the call.
///
/// Ported from `require_family_path` (`links.py:286-317`). Python's `urlsplit` also extracts a path
/// from a URL with any other scheme or a `//host` prefix; here such a value is not a path and is
/// rejected as outside the family.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` and the plain-path rule's message if `url` fails it
/// or contains a backslash, and otherwise with `"must be a path under <family> on the API host"`
/// (the family exactly as passed, `{id}` placeholders included).
///
/// # Examples
///
/// ```
/// use rusteero::links::require_family_path;
///
/// let ids = require_family_path(
///     "/2.2/networks/n1/profiles/p1",
///     "networks/{id}/profiles/{id}",
///     "",
///     "child",
/// )
/// .unwrap();
/// assert_eq!(ids, ["n1", "p1"]);
///
/// let err = require_family_path("/2.2/account", "networks/{id}", "", "network").unwrap_err();
/// assert_eq!(
///     err.to_string(),
///     "Validation error for 'network': must be a path under networks/{id} on the API host"
/// );
/// ```
pub fn require_family_path(
    url: &str,
    family: &str,
    suffix: &str,
    field: &str,
) -> Result<Vec<String>, Error> {
    require_plain_path(url, field)?;
    require_no_backslash(url, field)?;

    let mut pieces = Vec::new();
    for (index, part) in family.split(ID_PLACEHOLDER).enumerate() {
        if index > 0 {
            pieces.push(FamilyPiece::Id);
        }
        pieces.push(FamilyPiece::Literal(part));
    }
    pieces.push(FamilyPiece::Literal(suffix));

    let mut ids = Vec::new();
    let matched = raw_path(url)
        .strip_prefix('/')
        .and_then(|path| path.split_once('/'))
        .is_some_and(|(version, body)| {
            is_version_segment(version) && match_family(body, &pieces, &mut ids)
        });
    if !matched {
        return Err(Error::validation(
            field,
            format!("must be a path under {family} on the API host"),
        ));
    }
    Ok(ids.into_iter().map(str::to_owned).collect())
}

/// Resolves a bare ID or an API-returned path/URL to an absolute [`Url`], the single helper for
/// the id-or-url polymorphism domain modules encounter throughout the API.
///
/// `template` must contain exactly one `{id}` placeholder, e.g. `"networks/{id}/settings"`.
/// Interpreted as relative to `version`.
///
/// Ported from `resource_url` (`links.py:320-391`):
/// - An absolute `http(s)://` `id_or_url` is validated on-`host`, then `.rstrip('/') + suffix`
///   (the template text after `{id}`) is appended.
/// - A `/`-prefixed `id_or_url` is joined via [`join_api_path`], with the same suffix appended.
/// - A bare identifier is validated and substituted into the whole template, resolved against
///   `host/{version}`.
///
/// The text before `{id}`, trimmed of `/`, is the template's resource *family* (`networks` for
/// `"networks/{id}/settings"`, `entitlements/networks` for `"entitlements/networks/{id}/features"`).
/// When there is one, a caller-supplied path or URL must be exactly `/<version>/<family>/<id>`
/// ([`require_family_path`] with the field `"id_or_url"`), so an identifier of another family
/// cannot be redirected onto this template's endpoint (`/2.2/networks/123` with
/// `"eeros/{id}/reboot"`, or `/2.2/account` with `"networks/{id}/password"`). The caller's own
/// version segment is kept. Exactly one trailing `/` is tolerated and dropped before the suffix is
/// appended; the check sees the value as the caller wrote it, so two are rejected. A template with
/// nothing before `{id}` (`"{id}"`, `"{id}/schedules"`) names no family: its path or URL gets only
/// the host and plain-path checks.
///
/// # Errors
///
/// Returns [`Error::validation`] if `id_or_url` is empty, `template` does not contain exactly
/// one `{id}` placeholder, or the id/path/URL itself fails validation. A bad path reports
/// `field: "id_or_url"`, a bad absolute URL `field: "url"` (see the plain-path rule and the
/// origin rule in the module docs), and a bare id `field: "id"`.
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
    let Some((prefix, suffix)) = template.split_once(ID_PLACEHOLDER) else {
        // Unreachable: the `matches(..).count() != 1` check above already returned `Err` for
        // zero occurrences, so a single occurrence is always found here. Written as a second
        // real check (rather than `.expect(..)`) so this function can never panic, keeping the
        // "every fallible operation returns `Result`" convention exact.
        return Err(Error::validation(
            "template",
            "must contain exactly one {id} placeholder",
        ));
    };
    let family = prefix.trim_matches('/');
    // The family check reads the raw, untrimmed value. A template that names no family confines
    // nothing.
    let confine = |raw: &str| {
        if family.is_empty() {
            return Ok(());
        }
        require_family_path(raw, &format!("{family}/{ID_PLACEHOLDER}"), "", "id_or_url").map(drop)
    };

    if http_scheme_prefix_len(id_or_url).is_some() {
        let validated = validate_absolute_url(id_or_url, host)?;
        confine(id_or_url)?;
        let joined = format!("{}{suffix}", validated.as_str().trim_end_matches('/'));
        return Url::parse(&joined)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")));
    }

    if id_or_url.starts_with('/') {
        validate_link_path(id_or_url, "id_or_url")?;
        confine(id_or_url)?;
        let joined = join_api_path(host, id_or_url)?;
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
    // `api_endpoint(version)` (`links.py:391`), which is the same thing there because Python
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
/// Ported from `child_url` (`links.py:438-456`).
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
/// Ported from `sub_resource_url` (`links.py:459-495`).
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
/// [`rewrite_version`] and [`require_family_path`], so the two agree on what a version segment
/// is.
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
/// [`ApiVersion::V2_2`](crate::routes::ApiVersion::V2_2) becomes `/2.2/networks/n/2.3/support`. Everything else is left exactly
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
        child_url, is_version_segment, join_api_path, origin_mismatch, require_family_path,
        resolve_link, resource_url, rewrite_version, self_url, sub_resource_url,
        validate_identifier,
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

    /// The text of [`host`], for building absolute URLs.
    const MOCK: &str = "http://mock.test:1234";

    /// The one message every origin failure of the absolute-URL validator reports against
    /// [`host`].
    const ORIGIN_MESSAGE: &str =
        "must be an absolute http URL on mock.test with no userinfo and no other port";

    /// The `(field, message)` of a validation error; panics on any other outcome.
    fn rejection<T: std::fmt::Debug>(result: Result<T, Error>) -> (String, String) {
        match result {
            Err(Error::Validation { field, message, .. }) => (field, message),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    /// Resolves `supplied` against `template` on [`host`] at `2.2` and returns the URL as text.
    fn resolved(supplied: &str, template: &str) -> String {
        resource_url(&host(), supplied, template, ApiVersion::V2_2)
            .unwrap_or_else(|err| panic!("{supplied:?} with {template:?} must resolve: {err:?}"))
            .to_string()
    }

    /// The `(field, message)` of rejecting `supplied` against `template` on [`host`] at `2.2`.
    fn rejected(supplied: &str, template: &str) -> (String, String) {
        rejection(resource_url(&host(), supplied, template, ApiVersion::V2_2))
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
        assert!(matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "url" && message == ORIGIN_MESSAGE
        ));
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

    // ===================== query/fragment rejection =====================

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
        assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
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
        assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
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

    // ===================== absolute-URL origin rule (userinfo, port, authority) =====================

    #[test]
    fn resource_url_rejects_userinfo_on_the_configured_host() {
        for authority in [
            "user:pass@mock.test:1234",
            "user@mock.test:1234",
            ":pass@mock.test:1234",
            "@mock.test:1234",
            "user:pass@mock.test:9999",
        ] {
            for template in ["networks/{id}", "{id}"] {
                let (field, message) =
                    rejected(&format!("http://{authority}/2.2/networks/100"), template);
                assert_eq!(field, "url", "{authority} {template}");
                assert_eq!(message, ORIGIN_MESSAGE, "{authority} {template}");
                assert!(!message.contains("pass"));
            }
        }
    }

    #[test]
    fn resource_url_rejects_another_port_on_the_configured_host() {
        for authority in ["mock.test:9999", "mock.test", "mock.test:80", "mock.test:0"] {
            let (field, message) = rejected(
                &format!("http://{authority}/2.2/networks/100"),
                "networks/{id}",
            );
            assert_eq!(field, "url", "{authority}");
            assert_eq!(message, ORIGIN_MESSAGE, "{authority}");
        }
    }

    #[test]
    fn resource_url_unparseable_authority_is_an_origin_validation_error() {
        for authority in [
            "mock.test:abc",
            "mock.test:99999",
            "mock.test:1234:1234",
            "[::1",
            "mock.test\u{ff1a}99",
        ] {
            let (field, message) = rejected(
                &format!("http://{authority}/2.2/networks/100"),
                "networks/{id}",
            );
            assert_eq!(field, "url", "{authority}");
            assert_eq!(message, ORIGIN_MESSAGE, "{authority}");
        }
    }

    #[test]
    fn resource_url_rejects_backslash_percent_and_non_ascii_authorities() {
        for authority in [
            "evil.example\\@mock.test:1234",
            "mock.test:1234\\@evil.example",
            "mock.test\u{ff0f}@evil.example",
            "\u{ff4d}ock.test:1234",
            "%6Dock.test:1234",
            "mock.te%73t:1234",
            "mo\u{e7}k.test:1234",
        ] {
            let (field, message) = rejected(
                &format!("http://{authority}/2.2/networks/100"),
                "networks/{id}",
            );
            assert_eq!(field, "url", "{authority}");
            assert_eq!(message, ORIGIN_MESSAGE, "{authority}");
        }
    }

    #[test]
    fn resource_url_host_must_be_spelled_the_way_the_configured_host_is() {
        // The parser folds `127.1` and `0x7f.0.0.1` into `127.0.0.1`; a raw comparison does not.
        let loopback = Url::parse("http://127.0.0.1:1234").unwrap();
        for authority in ["127.1:1234", "0x7f.0.0.1:1234", "2130706433:1234"] {
            let err = resource_url(
                &loopback,
                &format!("http://{authority}/2.2/networks/100"),
                "networks/{id}",
                ApiVersion::V2_2,
            );
            let (field, message) = rejection(err);
            assert_eq!(field, "url", "{authority}");
            assert_eq!(
                message,
                "must be an absolute http URL on 127.0.0.1 with no userinfo and no other port"
            );
        }
        let url = resource_url(
            &loopback,
            "http://127.0.0.1:1234/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(url.as_str(), "http://127.0.0.1:1234/2.2/networks/100");
    }

    #[test]
    fn resource_url_accepts_an_ipv6_host_spelled_canonically() {
        let loopback = Url::parse("http://[::1]:8080").unwrap();
        let url = resource_url(
            &loopback,
            "http://[::1]:8080/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(url.as_str(), "http://[::1]:8080/2.2/networks/100");
        let err = resource_url(
            &loopback,
            "http://[0:0:0:0:0:0:0:1]:8080/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        );
        assert_eq!(rejection(err).0, "url");
    }

    #[test]
    fn resource_url_explicit_default_port_is_the_same_origin() {
        let secure = Url::parse("https://mock.test").unwrap();
        let url = resource_url(
            &secure,
            "https://mock.test:443/2.2/networks/n",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(url.as_str(), "https://mock.test/2.2/networks/n");
        let url = resource_url(
            &secure,
            "https://mock.test:443/2.2/networks/n",
            "networks/{id}/settings",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(url.as_str(), "https://mock.test/2.2/networks/n/settings");

        let plain = Url::parse("http://mock.test").unwrap();
        let url = resource_url(
            &plain,
            "http://mock.test:80/2.2/networks/n",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(url.as_str(), "http://mock.test/2.2/networks/n");
    }

    #[test]
    fn resource_url_scheme_and_host_case_do_not_matter() {
        assert_eq!(
            resolved("HTTP://MOCK.TEST:1234/2.2/networks/100", "networks/{id}"),
            "http://mock.test:1234/2.2/networks/100"
        );
    }

    #[test]
    fn resource_url_uppercase_scheme_is_accepted_and_normalised() {
        assert_eq!(
            resolved("HTTP://mock.test:1234/2.2/eeros/9", "eeros/{id}/reboot"),
            "http://mock.test:1234/2.2/eeros/9/reboot"
        );
    }

    #[test]
    fn resource_url_uppercase_scheme_off_host_is_validated_as_absolute() {
        let (field, message) = rejected("HTTPS://evil.example/x", "networks/{id}");
        assert_eq!((field.as_str(), message.as_str()), ("url", ORIGIN_MESSAGE));
    }

    // ===================== plain-path rule: whitespace and control characters =====================

    /// Whitespace and control characters a parser may strip, encode or keep: space, tab, CR, LF,
    /// NUL, VT, DEL, NBSP, line separator and NEL.
    const CONTROLS: [&str; 10] = [
        "\n", "\t", "\r", " ", "\u{0}", "\u{b}", "\u{7f}", "\u{a0}", "\u{2028}", "\u{85}",
    ];

    #[test]
    fn whitespace_and_control_characters_are_rejected_anywhere_in_a_path_or_url() {
        for control in CONTROLS {
            for path in [
                format!("/2.2/eeros/9{control}9"),
                format!("/2.2/eeros/99{control}"),
                format!("/2.2{control}/eeros/99"),
            ] {
                for (supplied, field) in [
                    (path.clone(), "id_or_url"),
                    (format!("{MOCK}{path}"), "url"),
                ] {
                    let (got_field, message) = rejected(&supplied, "eeros/{id}/reboot");
                    assert_eq!(got_field, field, "{supplied:?}");
                    assert_eq!(
                        message, "must not contain whitespace or control characters",
                        "{supplied:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn unsafe_absolute_url_is_rejected_for_a_template_without_a_family() {
        for hostile in [
            format!("{MOCK}/2.2/networks/n\n/x"),
            format!("{MOCK}/2.2/networks/n\t/x"),
            format!("{MOCK}/2.2/networks/n /x"),
            format!("{MOCK}/2.2/networks/n\u{0}"),
            format!("{MOCK}/2.2/networks/n/../x"),
            format!("{MOCK}/2.2/networks/n?x=1"),
            format!("{MOCK}/2.2/networks/n#frag"),
        ] {
            assert_eq!(rejected(&hostile, "{id}").0, "url", "{hostile:?}");
        }
    }

    #[test]
    fn published_links_reject_whitespace_and_control_characters() {
        for bad in [
            "/2.2/networks/n/dev\nices",
            "/2.2/networks/n/dev\tices",
            "/2.2/networks/n/dev ices",
            "/2.2/networks/n/dev\u{0}ices",
            "/2.2/networks/n/devices\r\n",
        ] {
            let by_name = resolve_link(&host(), &json!({"resources": {"devices": bad}}), "devices");
            assert_eq!(
                rejection(by_name),
                (
                    "link".to_owned(),
                    "must not contain whitespace or control characters".to_owned()
                ),
                "{bad:?}"
            );
            let own = self_url(&host(), &json!({"url": bad}));
            assert_eq!(rejection(own).0, "link", "{bad:?}");
        }
    }

    // ===================== plain-path rule: query, fragment and dot segments =====================

    #[test]
    fn dot_segments_are_rejected_in_caller_paths_and_urls() {
        for bad in [
            "/2.2/eeros/9/../../networks/123",
            "/2.2/eeros/%2e%2e/%2e%2e/networks/1",
            "/2.2/eeros/%2E%2E",
            "/2.2/eeros/..",
            "/2.2/eeros/.",
            "/2.2/eeros/.%2e",
            "/2.2/eeros/%2e.",
        ] {
            for (supplied, field) in [
                (bad.to_owned(), "id_or_url"),
                (format!("{MOCK}{bad}"), "url"),
            ] {
                assert_eq!(
                    rejected(&supplied, "eeros/{id}/reboot"),
                    (field.to_owned(), "must not contain dot segments".to_owned()),
                    "{supplied:?}"
                );
            }
        }
    }

    #[test]
    fn published_links_reject_dot_segments_but_allow_dots_inside_a_segment() {
        for bad in [
            "/2.2/networks/n/../account",
            "/2.2/networks/n/..",
            "/2.2/networks/n/./x",
            "/2.2/networks/n/%2e%2e/account",
            "/2.2/networks/n/%2E/x",
            "/2.2/networks/n/.%2e/x",
        ] {
            let by_name = resolve_link(&host(), &json!({"resources": {"x": bad}}), "x");
            assert_eq!(
                rejection(by_name),
                (
                    "link".to_owned(),
                    "must not contain dot segments".to_owned()
                ),
                "{bad:?}"
            );
            assert_eq!(rejection(self_url(&host(), &json!({"url": bad}))).0, "link");
        }
        let link = "/2.2/networks/n.1/devices/aa.bb";
        let by_name = resolve_link(&host(), &json!({"resources": {"d": link}}), "d");
        assert_eq!(
            by_name.unwrap().unwrap().as_str(),
            "http://mock.test:1234/2.2/networks/n.1/devices/aa.bb"
        );
        let own = self_url(&host(), &json!({"url": link}));
        assert_eq!(
            own.unwrap().unwrap().as_str(),
            "http://mock.test:1234/2.2/networks/n.1/devices/aa.bb"
        );
    }

    #[test]
    fn shape_errors_are_reported_before_the_origin_error_in_python_order() {
        assert_eq!(
            rejected("https://evil.example/x?y=1", "networks/{id}"),
            (
                "url".to_owned(),
                "must not carry a query or fragment".to_owned()
            )
        );
        assert_eq!(
            rejected("https://evil.example/../x", "networks/{id}"),
            ("url".to_owned(), "must not contain dot segments".to_owned())
        );
        // Control characters first, then query or fragment, then dot segments.
        assert_eq!(
            rejected("/2.2/a\n/../b?x", "{id}").1,
            "must not contain whitespace or control characters"
        );
        assert_eq!(
            rejected("/2.2/a/../b?x", "{id}").1,
            "must not carry a query or fragment"
        );
        assert_eq!(
            rejected("/2.2/a/../b", "{id}").1,
            "must not contain dot segments"
        );
    }

    #[test]
    fn an_empty_query_or_fragment_marker_is_rejected_too() {
        for bad in [
            "/2.2/networks/100?",
            "/2.2/networks/100#",
            "/2.2/networks/100?#",
        ] {
            for supplied in [bad.to_owned(), format!("{MOCK}{bad}")] {
                assert_eq!(
                    rejected(&supplied, "networks/{id}").1,
                    "must not carry a query or fragment",
                    "{supplied:?}"
                );
            }
        }
    }

    // ===================== backslash (stricter than Python) =====================

    #[test]
    fn a_backslash_is_rejected_in_paths_links_and_urls() {
        for bad in [
            "/2.2/networks/n\\..\\account",
            "/2.2/networks\\n1",
            "/2.2/eeros/9\\",
        ] {
            let (field, message) = rejected(bad, "{id}");
            assert_eq!(field, "id_or_url", "{bad:?}");
            assert_eq!(message, "must not contain a backslash", "{bad:?}");
            let by_name = resolve_link(&host(), &json!({"resources": {"x": bad}}), "x");
            assert_eq!(
                rejection(by_name),
                ("link".to_owned(), "must not contain a backslash".to_owned()),
                "{bad:?}"
            );
            assert_eq!(rejection(self_url(&host(), &json!({"url": bad}))).0, "link");
        }
        // Without a leading slash it is not a path at all, only a bad bare id.
        assert_eq!(rejected("\\2.2\\eeros\\9", "{id}").0, "id");
        // In the path of an absolute URL it is a path message; in the authority it is an origin
        // failure.
        assert_eq!(
            rejected(&format!("{MOCK}/2.2/networks/n\\..\\account"), "{id}"),
            ("url".to_owned(), "must not contain a backslash".to_owned())
        );
        assert_eq!(
            rejected(&format!("{MOCK}\\@evil.example/2.2/networks/n"), "{id}"),
            ("url".to_owned(), ORIGIN_MESSAGE.to_owned())
        );
    }

    // ===================== error field naming =====================

    #[test]
    fn error_field_names_the_callers_argument_or_the_envelope_link() {
        for bad in [
            "/2.2/networks/n/../account",
            "/2.2/networks/n?x=1",
            "//evil.example/x",
            "/2.2/n\n/x",
        ] {
            assert_eq!(rejected(bad, "networks/{id}").0, "id_or_url", "{bad:?}");
            let by_name = resolve_link(&host(), &json!({"resources": {"x": bad}}), "x");
            assert_eq!(rejection(by_name).0, "link", "{bad:?}");
            assert_eq!(
                rejection(self_url(&host(), &json!({"url": bad}))).0,
                "link",
                "{bad:?}"
            );
        }
        assert_eq!(
            rejected("//evil.example/x", "networks/{id}"),
            (
                "id_or_url".to_owned(),
                "must be a host-relative API path".to_owned()
            )
        );
        // An absolute URL reports `url`, a bare id `id`, a wrong family `id_or_url`.
        assert_eq!(rejected("https://evil.example/x", "networks/{id}").0, "url");
        assert_eq!(rejected("a b", "networks/{id}").0, "id");
        assert_eq!(rejected("/2.2/account", "networks/{id}").0, "id_or_url");
    }

    // ===================== require_family_path =====================

    const SCHEDULE_FAMILY: &str = "networks/{id}/profiles/{id}/schedules/{id}";

    fn family(url: &str, family: &str, suffix: &str) -> Result<Vec<String>, Error> {
        require_family_path(url, family, suffix, "schedule")
    }

    #[test]
    fn require_family_path_returns_the_ids_in_placeholder_order() {
        assert_eq!(
            family(
                "/2.2/networks/n1/profiles/p1/schedules/s1",
                SCHEDULE_FAMILY,
                ""
            )
            .unwrap(),
            ["n1", "p1", "s1"]
        );
        assert_eq!(
            family(
                "/2.3/networks/n1/profiles/p1/schedules",
                "networks/{id}/profiles/{id}",
                "/schedules"
            )
            .unwrap(),
            ["n1", "p1"]
        );
        assert_eq!(
            family("/2.2/eeros/aa:bb:cc:11:22:33", "eeros/{id}", "").unwrap(),
            ["aa:bb:cc:11:22:33"]
        );
        assert!(
            family("/10.20/networks", "networks", "")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn require_family_path_accepts_one_trailing_slash_only() {
        assert_eq!(
            family(
                "/2.2/networks/n1/profiles/p1/schedules/s1/",
                SCHEDULE_FAMILY,
                ""
            )
            .unwrap(),
            ["n1", "p1", "s1"]
        );
        assert!(
            family(
                "/2.2/networks/n1/profiles/p1/schedules/s1//",
                SCHEDULE_FAMILY,
                ""
            )
            .is_err()
        );
    }

    #[test]
    fn require_family_path_rejects_anything_outside_the_family() {
        for bad in [
            "/2.2/networks/n1/profiles/p1/schedules/s1/extra",
            "/2.2/networks/n1/profiles/p1/schedules",
            "/2.2/networks/n1/profiles/p1/schedules/",
            "/2.2/networks/n1/profiles/p1",
            "/2.2/networks/n1/profiles//schedules/s1",
            "/x.y/networks/n1/profiles/p1/schedules/s1",
            "/2/networks/n1/profiles/p1/schedules/s1",
            "/2.2.2/networks/n1/profiles/p1/schedules/s1",
            "/\u{662}.2/networks/n1/profiles/p1/schedules/s1",
            "/networks/n1/profiles/p1/schedules/s1",
            "2.2/networks/n1/profiles/p1/schedules/s1",
            "/2.2/networks/_p1/profiles/p1/schedules/s1",
            "/2.2/networks/n1/profiles/-p1/schedules/s1",
            "/2.2/networks/n1/profiles/.p1/schedules/s1",
            "/2.2/networks/n1/profiles/p%41/schedules/s1",
            "/2.2/networks/n1/profiles/p\u{e9}/schedules/s1",
            "/2.2/networks/n1/PROFILES/p1/schedules/s1",
            "",
            "/",
        ] {
            let (field, message) = rejection(family(bad, SCHEDULE_FAMILY, ""));
            assert_eq!(field, "schedule", "{bad:?}");
            assert_eq!(
                message,
                "must be a path under networks/{id}/profiles/{id}/schedules/{id} on the API host",
                "{bad:?}"
            );
        }
    }

    #[test]
    fn require_family_path_applies_the_plain_path_rule_first_with_the_given_field() {
        for (bad, message) in [
            (
                "/2.2/networks/n1\n",
                "must not contain whitespace or control characters",
            ),
            ("/2.2/networks/n1?x=1", "must not carry a query or fragment"),
            ("/2.2/networks/n1#f", "must not carry a query or fragment"),
            ("/2.2/networks/..", "must not contain dot segments"),
            ("/2.2/networks/%2e%2e", "must not contain dot segments"),
            ("/2.2/networks\\n1", "must not contain a backslash"),
        ] {
            assert_eq!(
                rejection(require_family_path(bad, "networks/{id}", "", "network")),
                ("network".to_owned(), message.to_owned()),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn require_family_path_reads_the_path_of_an_http_url_and_ignores_its_host() {
        for url in [
            "HTTPS://mock.test:1234/2.2/eeros/9",
            "http://mock.test:1234/2.2/eeros/9/",
            "https://anywhere.example/2.2/eeros/9",
        ] {
            assert_eq!(family(url, "eeros/{id}", "").unwrap(), ["9"], "{url}");
        }
        for url in [
            "http://mock.test:1234",
            "http://mock.test:1234/",
            "http://mock.test:1234/2.2/networks/9",
            // Not an `http(s)` URL, so not a path either.
            "ftp://mock.test/2.2/eeros/9",
            "//mock.test/2.2/eeros/9",
        ] {
            assert!(family(url, "eeros/{id}", "").is_err(), "{url}");
        }
    }

    #[test]
    fn require_family_path_matches_a_literal_suffix_and_multi_segment_family() {
        let ids = family(
            "/2.2/networks/n1/insights/devices/aa:bb:cc:11:22:33/history",
            "networks/{id}/insights/devices/{id}",
            "/history",
        )
        .unwrap();
        assert_eq!(ids, ["n1", "aa:bb:cc:11:22:33"]);
        assert!(
            family(
                "/2.2/networks/n1/insights/devices/aa:bb:cc:11:22:33",
                "networks/{id}/insights/devices/{id}",
                "/history",
            )
            .is_err()
        );
        assert!(
            family(
                "/2.2/networks/n1/insights/profiles/p_1",
                "networks/{id}/insights/devices/{id}",
                "",
            )
            .is_err()
        );
    }

    #[test]
    fn require_family_path_message_keeps_the_family_as_written() {
        let err = require_family_path("/2.2/account", "eeros/{id}", "", "child").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Validation error for 'child': must be a path under eeros/{id} on the API host"
        );
    }

    // ===================== resource_url family confinement =====================

    #[test]
    fn resource_url_rejects_a_path_outside_the_templates_family() {
        for (supplied, template) in [
            ("/2.2/networks/123", "eeros/{id}/reboot"),
            ("/2.2/account", "networks/{id}/password"),
            ("/2.2/eeros/9/../../networks/123", "eeros/{id}/reboot"),
            ("/2.2/eeros/9?x=1", "eeros/{id}/reboot"),
            ("/2.2/eeros/9#frag", "eeros/{id}/reboot"),
            ("/2.2/eeros/%2e%2e/%2e%2e/networks/1", "eeros/{id}/reboot"),
            ("/2.2/eeros/%2E%2E", "eeros/{id}/reboot"),
            ("/2.2/eeros/9/extra", "eeros/{id}/reboot"),
            ("/2.2/eeros", "eeros/{id}/reboot"),
            ("/2.2/eeros/", "eeros/{id}/reboot"),
            ("/2.2/eeros//9", "eeros/{id}/reboot"),
            ("/2.2/eeros/..", "eeros/{id}/reboot"),
            ("/2.2/eeros/.", "eeros/{id}/reboot"),
            ("/2.2/eeros/9//", "eeros/{id}/reboot"),
            ("/2.2/eeros/9%2f..", "eeros/{id}/reboot"),
            ("/eeros/9", "eeros/{id}/reboot"),
            ("/x.y/eeros/9", "eeros/{id}/reboot"),
            ("/2.2/networks/n1", "entitlements/networks/{id}/features"),
            (
                "/2.2/entitlements/networks/n1/features",
                "entitlements/networks/{id}/features",
            ),
            ("/2.2/networks/n1/eeros/9", "networks/{id}/settings"),
        ] {
            for url_form in [false, true] {
                let supplied = if url_form {
                    format!("{MOCK}{supplied}")
                } else {
                    supplied.to_owned()
                };
                let (field, _) = rejected(&supplied, template);
                // A wrong-family path reports the caller's argument; a shape violation in an
                // absolute URL reports `url`.
                assert!(
                    field == "id_or_url" || (url_form && field == "url"),
                    "{supplied:?} {template:?}: {field}"
                );
            }
        }
    }

    #[test]
    fn resource_url_reports_the_family_message_under_id_or_url() {
        for supplied in ["/2.2/account".to_owned(), format!("{MOCK}/2.2/account")] {
            assert_eq!(
                rejected(&supplied, "networks/{id}/password"),
                (
                    "id_or_url".to_owned(),
                    "must be a path under networks/{id} on the API host".to_owned()
                ),
                "{supplied}"
            );
        }
        assert_eq!(
            rejected("/2.2/networks/n1", "entitlements/networks/{id}/features").1,
            "must be a path under entitlements/networks/{id} on the API host"
        );
    }

    #[test]
    fn resource_url_accepts_a_path_in_the_templates_family_and_keeps_its_version() {
        for (supplied, template, expected) in [
            (
                "9",
                "eeros/{id}/reboot",
                "http://mock.test:1234/2.2/eeros/9/reboot",
            ),
            (
                "/2.2/networks/n1",
                "networks/{id}/settings",
                "http://mock.test:1234/2.2/networks/n1/settings",
            ),
            (
                "/2.3/networks/n1",
                "networks/{id}/settings",
                "http://mock.test:1234/2.3/networks/n1/settings",
            ),
            (
                "/2.2/eeros/9",
                "eeros/{id}/reboot",
                "http://mock.test:1234/2.2/eeros/9/reboot",
            ),
            (
                "http://mock.test:1234/2.2/eeros/9",
                "eeros/{id}/reboot",
                "http://mock.test:1234/2.2/eeros/9/reboot",
            ),
            (
                "http://mock.test:1234/2.2/eeros/9/",
                "eeros/{id}/reboot",
                "http://mock.test:1234/2.2/eeros/9/reboot",
            ),
            (
                "/2.2/eeros/9/",
                "eeros/{id}/reboot",
                "http://mock.test:1234/2.2/eeros/9/reboot",
            ),
            (
                "/2.2/eeros/aa:bb:cc:11:22:33",
                "eeros/{id}",
                "http://mock.test:1234/2.2/eeros/aa:bb:cc:11:22:33",
            ),
            (
                "/2.2/entitlements/networks/n1",
                "entitlements/networks/{id}/features",
                "http://mock.test:1234/2.2/entitlements/networks/n1/features",
            ),
        ] {
            assert_eq!(
                resolved(supplied, template),
                expected,
                "{supplied:?} {template:?}"
            );
        }
    }

    #[test]
    fn resource_url_tolerates_exactly_one_trailing_slash_when_the_template_has_a_family() {
        assert_eq!(
            resolved("/2.2/networks/n1/", "networks/{id}/settings"),
            "http://mock.test:1234/2.2/networks/n1/settings"
        );
        assert!(
            rejected("/2.2/networks/n1//", "networks/{id}/settings")
                .1
                .starts_with("must be a path")
        );
        assert!(
            rejected(
                &format!("{MOCK}/2.2/networks/n1//"),
                "networks/{id}/settings"
            )
            .1
            .starts_with("must be a path")
        );
    }

    #[test]
    fn resource_url_without_a_family_strips_every_trailing_slash_and_checks_only_the_shape() {
        let path = "/2.2/networks/n1/profiles/p1/schedules/s1";
        assert_eq!(resolved(path, "{id}"), format!("{MOCK}{path}"));
        assert_eq!(
            resolved(&format!("{path}///"), "{id}"),
            format!("{MOCK}{path}")
        );
        assert_eq!(
            resolved(&format!("{path}//"), "{id}/schedules"),
            format!("{MOCK}{path}/schedules")
        );
        assert_eq!(
            resolved(&format!("{MOCK}{path}/"), "{id}"),
            format!("{MOCK}{path}")
        );
        assert_eq!(
            rejected("/2.2/networks/n1/../account", "{id}"),
            (
                "id_or_url".to_owned(),
                "must not contain dot segments".to_owned()
            )
        );
    }

    #[test]
    fn resource_url_family_is_the_text_before_the_placeholder_trimmed_of_slashes() {
        assert_eq!(
            resolved("/2.2/networks/n1", "/networks/{id}/settings"),
            "http://mock.test:1234/2.2/networks/n1/settings"
        );
        assert!(
            rejected("/2.2/eeros/9", "/networks/{id}/settings")
                .1
                .starts_with("must be a path")
        );
    }

    // ===================== bare identifiers =====================

    #[test]
    fn resource_url_rejects_trailing_newline_and_control_characters_in_a_bare_id() {
        for bad in ["abc\n", "abc\t", "abc\r\n", "ab\u{0}c", "-abc", ".abc"] {
            let (field, message) = rejected(bad, "networks/{id}/password");
            assert_eq!(field, "id", "{bad:?}");
            assert_eq!(
                message, "must be a single path segment identifier",
                "{bad:?}"
            );
        }
    }

    // ===================== published links are not confined =====================

    #[test]
    fn published_links_are_not_confined_to_a_family_or_a_version() {
        for link in ["/2.2/account", "/2.3/networks/other/eeros", "/2.1/x/y"] {
            let expected = format!("{MOCK}{link}");
            let by_name = resolve_link(&host(), &json!({"resources": {"any": link}}), "any");
            assert_eq!(by_name.unwrap().unwrap().as_str(), expected);
            let own = self_url(&host(), &json!({"url": link}));
            assert_eq!(own.unwrap().unwrap().as_str(), expected);
        }
    }

    #[test]
    fn a_sub_resource_link_on_another_version_still_wins() {
        let parent = json!({"resources": {"forwards": "/2.3/networks/n1/forwards"}});
        let url = sub_resource_url(
            &host(),
            "n1",
            "networks/{id}/forwards",
            "forwards",
            Some(&parent),
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.3/networks/n1/forwards"
        );
    }
}
