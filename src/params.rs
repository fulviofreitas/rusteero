//! Shared query-parameter validation and identifier-resolution helpers.
//!
//! Ported from `eero-api`'s `src/eero/api/_params.py` at `v8.0.4`. Small, dependency-free
//! helpers shared by more than one domain module, so that validation rules and
//! identifier-resolution rules have exactly one implementation each rather than one copy per
//! module.

use regex_free_matchers::path_matches_nested_family;
use serde_json::Value;
use url::Url;

use crate::error::Error;
use crate::links::{child_url, resolve_link, resource_url, self_url};
use crate::routes::ApiVersion;
use crate::util::id_from_url;

/// Valid values for a `cadence` query/body parameter, shared by every family that buckets a time
/// series into "daily" or "hourly" points (data usage, insights, data-usage report settings).
///
/// Ported from `CADENCE_VALUES` (`_params.py:25`).
pub const CADENCE_VALUES: &[&str] = &["daily", "hourly"];

/// Renders a single value the way Python's `repr()` renders a `str`: single-quoted, verbatim.
///
/// Every value this crate ever formats this way (mode names, actions, cadences, IP literals) is
/// plain ASCII with no embedded quote or backslash, so this is always exactly Python's `repr()`
/// output for that value — it is not a general-purpose Python `repr()` implementation.
#[must_use]
pub fn py_quote(value: &str) -> String {
    format!("'{value}'")
}

/// Renders `items` as Python's `repr()` renders a `list` of `str`, **sorted** ascending first —
/// every call site that formats a vocabulary this way ports a Python `sorted(...)` call (e.g.
/// `eeros.py:681-683`), so the sort is not optional.
///
/// # Examples
///
/// ```
/// # use rusteero::params::py_list;
/// assert_eq!(py_list(&["b", "a"]), "['a', 'b']");
/// ```
#[must_use]
pub fn py_list(items: &[&str]) -> String {
    let mut sorted: Vec<&str> = items.to_vec();
    sorted.sort_unstable();
    let joined = sorted
        .iter()
        .map(|item| py_quote(item))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

/// Renders `items` as Python's `repr()` renders a `tuple` of `str`, in the exact order given —
/// every call site that formats a vocabulary this way ports a Python `tuple(...)` call over a
/// sequence that is already in the order the message must show (e.g. `_params.py:47-50`'s
/// `tuple(allowed)`), so, unlike [`py_list`], this never sorts.
///
/// # Examples
///
/// ```
/// # use rusteero::params::py_tuple;
/// assert_eq!(py_tuple(&["daily", "hourly"]), "('daily', 'hourly')");
/// ```
#[must_use]
pub fn py_tuple(items: &[&str]) -> String {
    let joined = items
        .iter()
        .map(|item| py_quote(item))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({joined})")
}

/// Validates a `cadence` value against its endpoint's accepted buckets.
///
/// Ported from `validate_cadence` (`_params.py:28-53`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "cadence"` if `value` is not one of `allowed`.
pub fn validate_cadence<'a>(value: &'a str, allowed: &[&str]) -> Result<&'a str, Error> {
    if !allowed.contains(&value) {
        return Err(Error::validation(
            "cadence",
            format!(
                "must be one of {}, got {}",
                py_tuple(allowed),
                py_quote(value)
            ),
        ));
    }
    Ok(value)
}

/// Resolves a network identifier to its absolute base [`Url`].
///
/// The network's own URL ([`self_url`]) is preferred whenever `parent` is supplied and carries
/// one; otherwise the URL is built from `network` — a bare ID, a host-relative path, or an
/// absolute API-host URL — exactly as [`resource_url`] does.
///
/// Ported from `resolve_network_url` (`_params.py:59-98`).
///
/// # Errors
///
/// Returns [`Error::validation`] as [`resource_url`].
pub fn resolve_network_url(
    host: &Url,
    network: &str,
    parent: Option<&Value>,
    version: ApiVersion,
) -> Result<Url, Error> {
    if let Some(parent) = parent
        && let Some(resolved) = self_url(host, parent)?
    {
        return Ok(resolved);
    }
    resource_url(host, network, "networks/{id}", version)
}

/// Resolves a two-level nested resource URL — `networks/{network}/{prefix}/{child}{suffix}` —
/// without double-`str.format`-ing either id.
///
/// Ported from `resolve_nested_url` (`_params.py:101-169`).
///
/// # Errors
///
/// Returns [`Error::validation`] if `child` is not a non-empty string, is a bare id that is not
/// a single path segment, or is a path/URL that does not belong to the addressed network's
/// `prefix` family. As [`resolve_network_url`] for an invalid `network`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_nested_url(
    host: &Url,
    network: &str,
    child: &str,
    prefix: &str,
    suffix: &str,
    link: Option<&str>,
    parent: Option<&Value>,
    version: ApiVersion,
) -> Result<Url, Error> {
    if let (Some(parent), Some(link)) = (parent, link)
        && let Some(resolved) = resolve_link(host, parent, link)?
    {
        return Ok(resolved);
    }

    if child.starts_with("http://") || child.starts_with("https://") || child.starts_with('/') {
        let template = format!("{{id}}{suffix}");
        let resolved = resource_url(host, child, &template, version)?;
        return require_nested_family(&resolved, network, prefix, suffix);
    }
    if child.is_empty() {
        return Err(Error::validation("child", "must be a non-empty string"));
    }

    let network_url = resolve_network_url(host, network, None, version)?;
    let collection = format!("{}/{prefix}", network_url.as_str().trim_end_matches('/'));
    let collection_url = Url::parse(&collection)
        .map_err(|err| Error::validation("child", format!("not a valid URL: {err}")))?;
    let member = child_url(&collection_url, child)?;
    let with_suffix = format!("{}{suffix}", member.as_str());
    Url::parse(&with_suffix)
        .map_err(|err| Error::validation("child", format!("not a valid URL: {err}")))
}

/// Checks that a caller-supplied nested path names the expected resource family:
/// `/<version>/networks/<network>/<prefix>/<child><suffix>` with single-segment ids and nothing
/// after the suffix.
///
/// Ported from `_require_nested_family` (`_params.py:172-215`).
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "child"` if `url` carries a query or fragment,
/// does not match the expected path shape, or names a different network than `network`.
fn require_nested_family(
    url: &Url,
    network: &str,
    prefix: &str,
    suffix: &str,
) -> Result<Url, Error> {
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::validation(
            "child",
            "must not carry a query or fragment",
        ));
    }
    let Some((matched_network, _matched_child)) =
        path_matches_nested_family(url.path(), prefix, suffix)
    else {
        return Err(Error::validation(
            "child",
            format!("must be a path under networks/{{id}}/{prefix} on the API host"),
        ));
    };
    if !network.is_empty() {
        let expected_network = id_from_url(network)?;
        if matched_network != expected_network {
            return Err(Error::validation(
                "child",
                "must belong to the addressed network",
            ));
        }
    }
    Ok(url.clone())
}

/// Hand-written matcher for the fixed path shape
/// `/<digits>.<digits>/networks/<network>/<prefix>/<child><suffix>`, standing in for
/// `_params.py`'s single compiled regex (`_require_nested_family`, `_params.py:196-203`).
///
/// A regex crate is not among this crate's dependencies (no new dependencies without a stated
/// reason, per this port's conventions); the shape is fixed and small enough to check by hand.
/// `network` and `child` are each validated afterwards to be `[A-Za-z0-9._:-]+` — the same
/// character class the Python regex enforces inline.
mod regex_free_matchers {
    /// Returns `(network, child)` if `path` matches
    /// `/<digits>.<digits>/networks/<network>/<prefix>/<child><suffix>` with both `network` and
    /// `child` being single-segment identifiers (`[A-Za-z0-9._:-]+`), else `None`.
    pub(super) fn path_matches_nested_family<'a>(
        path: &'a str,
        prefix: &str,
        suffix: &str,
    ) -> Option<(&'a str, &'a str)> {
        let rest = path.strip_prefix('/')?;
        let (version, rest) = rest.split_once('/')?;
        if !is_version_segment(version) {
            return None;
        }
        let rest = rest.strip_prefix("networks/")?;
        let (network, rest) = rest.split_once('/')?;
        let rest = rest.strip_prefix(prefix)?;
        let rest = rest.strip_prefix('/')?;
        let rest = rest.strip_suffix(suffix)?;
        let child = rest;
        if !is_identifier_segment(network) || !is_identifier_segment(child) {
            return None;
        }
        Some((network, child))
    }

    fn is_version_segment(segment: &str) -> bool {
        let Some((major, minor)) = segment.split_once('.') else {
            return false;
        };
        !major.is_empty()
            && !minor.is_empty()
            && major.chars().all(|c| c.is_ascii_digit())
            && minor.chars().all(|c| c.is_ascii_digit())
    }

    fn is_identifier_segment(segment: &str) -> bool {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
    }
}

#[cfg(test)]
mod tests {
    use super::{CADENCE_VALUES, resolve_nested_url, resolve_network_url, validate_cadence};
    use crate::error::Error;
    use crate::routes::ApiVersion;
    use serde_json::json;
    use url::Url;

    fn host() -> Url {
        Url::parse("http://mock.test:1234").unwrap()
    }

    // ===================== TestValidateCadence =====================

    #[test]
    fn validate_cadence_accepts_default_values() {
        assert_eq!(validate_cadence("daily", CADENCE_VALUES).unwrap(), "daily");
        assert_eq!(
            validate_cadence("hourly", CADENCE_VALUES).unwrap(),
            "hourly"
        );
    }

    #[test]
    fn validate_cadence_rejects_unknown_value() {
        let err = validate_cadence("weekly", CADENCE_VALUES).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    }

    #[test]
    fn validate_cadence_honours_a_custom_allowed_set() {
        assert_eq!(
            validate_cadence("weekly", &["daily", "hourly", "weekly"]).unwrap(),
            "weekly"
        );
    }

    // ===================== TestResolveNetworkUrl =====================

    #[test]
    fn resolve_network_url_prefers_parents_self_url() {
        let parent = json!({"url": "/2.4/networks/100"});
        let url = resolve_network_url(&host(), "100", Some(&parent), ApiVersion::V2_2).unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.4/networks/100");
    }

    #[test]
    fn resolve_network_url_falls_back_to_bare_id() {
        let url = resolve_network_url(&host(), "100", None, ApiVersion::V2_2).unwrap();
        assert_eq!(url.as_str(), "http://mock.test:1234/2.2/networks/100");
    }

    // ===================== TestResolveNestedUrlChildValidation =====================

    #[test]
    fn resolve_nested_url_bare_child_id_is_appended() {
        let url = resolve_nested_url(
            &host(),
            "100",
            "p1",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/profiles/p1"
        );
    }

    #[test]
    fn resolve_nested_url_empty_child_is_rejected() {
        let err = resolve_nested_url(
            &host(),
            "100",
            "",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "child"));
    }

    #[test]
    fn resolve_nested_url_prefers_named_link_on_parent() {
        let parent = json!({"resources": {"schedules": "/2.5/networks/100/profiles/p1/schedules"}});
        let url = resolve_nested_url(
            &host(),
            "100",
            "p1",
            "profiles",
            "",
            Some("schedules"),
            Some(&parent),
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.5/networks/100/profiles/p1/schedules"
        );
    }

    // ===================== TestResolveNestedUrlPathChildFamily =====================

    #[test]
    fn resolve_nested_url_path_child_must_belong_to_the_addressed_network() {
        let err = resolve_nested_url(
            &host(),
            "100",
            "/2.2/networks/200/profiles/p1",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "child"));
    }

    #[test]
    fn resolve_nested_url_path_child_matching_network_is_accepted() {
        let url = resolve_nested_url(
            &host(),
            "100",
            "/2.2/networks/100/profiles/p1",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/profiles/p1"
        );
    }

    #[test]
    fn resolve_nested_url_path_child_wrong_family_is_rejected() {
        let err = resolve_nested_url(
            &host(),
            "100",
            "/2.2/account",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "child"));
    }

    #[test]
    fn resolve_nested_url_child_with_query_is_rejected() {
        let err = resolve_nested_url(
            &host(),
            "100",
            "/2.2/networks/100/profiles/p1?x=1",
            "profiles",
            "",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "child"));
    }

    #[test]
    fn resolve_nested_url_with_suffix() {
        let url = resolve_nested_url(
            &host(),
            "100",
            "aa:bb:cc",
            "insights/devices",
            "/history",
            None,
            None,
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.2/networks/100/insights/devices/aa:bb:cc/history"
        );
    }
}
