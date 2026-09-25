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
/// starts with `"//"` — none of which an API-published path ever does.
fn validate_link_path(link: &str) -> Result<&str, Error> {
    if link.is_empty() || link.contains("://") || link.starts_with("//") {
        return Err(Error::validation(
            "link",
            "must be a host-relative API path",
        ));
    }
    Ok(link)
}

/// Returns the `data` object of `parent`, without copying it, when `parent` looks like a full
/// response envelope (has both a `meta` key and an object `data` key); otherwise returns `parent`
/// unchanged.
///
/// Ported from `_as_data` (`links.py:108-124`).
fn as_data(parent: &Value) -> &Value {
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
/// Returns [`Error::validation`] with `field: "url"` if `url` does not parse, or its scheme or
/// hostname does not exactly match `host`'s.
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
    let base = version.base_url().trim_end_matches('/');
    let full = format!("{base}/{}", path.trim_start_matches('/'));
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
        child_url, join_api_path, resolve_link, resource_url, self_url, sub_resource_url,
        validate_identifier,
    };
    use crate::error::Error;
    use crate::routes::ApiVersion;
    use serde_json::json;
    use url::Url;

    fn host() -> Url {
        Url::parse("https://api-user.e2ro.com").unwrap()
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
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/1/eeros"
        );
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
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/networks/100");
    }

    #[test]
    fn resource_url_with_suffix_appends_after_the_id() {
        let url = resource_url(&host(), "100", "networks/{id}/settings", ApiVersion::V2_2).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/settings"
        );
    }

    #[test]
    fn resource_url_absolute_url_on_host_is_validated_and_suffix_appended() {
        let url = resource_url(
            &host(),
            "https://api-user.e2ro.com/2.3/networks/100",
            "networks/{id}/settings",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.3/networks/100/settings"
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
            "https://api-user.e2ro.com@evil.example/2.2/networks/100",
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
            "https://api-user.e2ro.com.evil.example/2.2/networks/100",
            "networks/{id}",
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
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
            "https://api-user.e2ro.com/2.3/networks/100/settings"
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
        assert!(url.as_str().starts_with("https://api-user.e2ro.com/2.3/"));
    }

    // ===================== child_url =====================

    #[test]
    fn child_url_appends_validated_child_id() {
        let base = Url::parse("https://api-user.e2ro.com/2.2/networks/100/blacklist").unwrap();
        let url = child_url(&base, "aa:bb:cc").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/blacklist/aa:bb:cc"
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
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/eeros"
        );
    }

    #[test]
    fn resolve_link_accepts_unwrapped_data_object() {
        let data = json!({"resources": {"eeros": "/2.2/networks/100/eeros"}});
        let url = resolve_link(&host(), &data, "eeros").unwrap().unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/100/eeros"
        );
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
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/networks/100");
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
            "https://api-user.e2ro.com/2.4/networks/100/settings"
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
            "https://api-user.e2ro.com/2.2/networks/100/settings"
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
            "https://api-user.e2ro.com/2.2/networks/100/settings"
        );
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
