//! Shared query-parameter validation and identifier-resolution helpers.
//!
//! Ported from `eero-api`'s `src/eero/api/_params.py` at `v8.4.1`. Small, dependency-free
//! helpers shared by more than one domain module, so that validation rules and
//! identifier-resolution rules have exactly one implementation each rather than one copy per
//! module.

use serde_json::Value;
use url::Url;

use crate::error::Error;
use crate::links::{child_url, require_family_path, resolve_link, resource_url, self_url};
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

/// Rejects an empty string for a required text parameter.
///
/// Ported from `require_non_empty_string` (`_params.py:53-69`). Python's form encoding renders
/// `None`, `False` or `0` as the literal text `"None"`, `"False"` or `"0"`, so the helper exists
/// to reject those instead of writing them to the network. In Rust the `&str` parameter already
/// rules out the "not a `str`" half, leaving only the emptiness check. The value is not trimmed,
/// so a whitespace-only string is accepted exactly as Python accepts it, and it is never
/// included in the error message.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` and the fixed message `"must be a non-empty
/// string"` if `value` is empty.
///
/// # Examples
///
/// ```
/// # use rusteero::params::require_non_empty_string;
/// assert!(require_non_empty_string("ssid", "Home").is_ok());
/// assert_eq!(
///     require_non_empty_string("ssid", "").unwrap_err().to_string(),
///     "Validation error for 'ssid': must be a non-empty string"
/// );
/// ```
pub fn require_non_empty_string(field: &str, value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::validation(field, "must be a non-empty string"));
    }
    Ok(())
}

/// Rejects an hour of day that is not an integer from 0 to 23.
///
/// Ported from `require_hour` (`_params.py:86-99`). Python must also reject `bool` (an `int`
/// subclass), `float`, `str` and `None` with the same message; the `i32` parameter makes all of
/// those unrepresentable, so only the range check carries over.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field` and the fixed message `"must be an int from 0 to
/// 23"` if `value` is outside `0..=23`.
///
/// # Examples
///
/// ```
/// # use rusteero::params::require_hour;
/// assert!(require_hour("hour", 23).is_ok());
/// assert_eq!(
///     require_hour("hour", 24).unwrap_err().to_string(),
///     "Validation error for 'hour': must be an int from 0 to 23"
/// );
/// ```
pub fn require_hour(field: &str, value: i32) -> Result<(), Error> {
    if !(0..=23).contains(&value) {
        return Err(Error::validation(field, "must be an int from 0 to 23"));
    }
    Ok(())
}

/// Resolves a network identifier to its absolute base [`Url`].
///
/// The network's own URL ([`self_url`]) is preferred whenever `parent` is supplied and carries
/// one; otherwise the URL is built from `network` — a bare ID, a host-relative path, or an
/// absolute API-host URL — exactly as [`resource_url`] does. A path or URL supplied here must name
/// exactly one network (`/<version>/networks/<id>`): `/2.2/account`, `/2.2/eeros/9` and
/// `/2.2/networks/n1/devices` are rejected. The parent's own published `url` is not confined.
///
/// Ported from `resolve_network_url` (`_params.py:102-140`).
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
/// Ported from `resolve_nested_url` (`_params.py:143-215`).
///
/// A `child` that is a path or URL (it starts with `http://`, `https://` or `/`) is resolved by
/// [`resource_url`] with the template `{id}<suffix>`, which applies the plain-path rule and drops
/// every trailing `/`, and is then required to name this `prefix` family under the addressed
/// network ([`require_family_path`], field `"child"`). A `child` that is a bare id is validated
/// as one path segment and appended to the network's `prefix` collection.
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
        return require_nested_family(child, resolved, network, prefix, suffix);
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
/// after the suffix, and that it belongs to the addressed network.
///
/// Ported from `_require_nested_family` (`_params.py:218-251`), which delegates the shape to
/// [`require_family_path`] (field `"child"`). `child` is the caller's raw string and `resolved`
/// what [`resource_url`] made of it: the family is checked on the raw text, with every trailing
/// `/` removed as [`resource_url`] removes it, so `.../profiles/p1//` is tolerated and resolves to
/// `.../profiles/p1`, while a dot segment, control character or percent-escape the URL parser
/// would have normalised away is still seen.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "child"` if the path does not match the expected
/// shape, or names a different network than `network`.
fn require_nested_family(
    child: &str,
    resolved: Url,
    network: &str,
    prefix: &str,
    suffix: &str,
) -> Result<Url, Error> {
    let raw = format!("{}{suffix}", child.trim_end_matches('/'));
    let family = format!("networks/{{id}}/{prefix}/{{id}}");
    let ids = require_family_path(&raw, &family, suffix, "child")?;
    if !network.is_empty() {
        let expected_network = id_from_url(network)?;
        if ids.first() != Some(&expected_network) {
            return Err(Error::validation(
                "child",
                "must belong to the addressed network",
            ));
        }
    }
    Ok(resolved)
}

/// The three-level family of a scheduled pause.
const SCHEDULE_FAMILY: &str = "networks/{id}/profiles/{id}/schedules/{id}";

/// Checks that a resolved scheduled-pause URL names the expected three-level resource family:
/// `/<version>/networks/<network>/profiles/<profile>/schedules/<schedule>`, with single-segment
/// ids and nothing after the final id.
///
/// Security finding 2 (phase-security-review): `ScheduleApi::update_schedule`/
/// `ScheduleApi::delete_schedule` used to accept *any* same-host path or absolute URL as a
/// schedule's own URL, as long as it resolved via [`crate::links::resource_url`]'s generic
/// `{id}`-template substitution — a bare identifier such as `"networks"`, an unrelated path such
/// as `"/2.2/networks/1"`, or a network's own envelope (whose `url` points at the network, not a
/// schedule) would all resolve to *some* URL and then be sent a `PUT`/`DELETE` without ever being
/// checked to actually be a schedule. This closes that gap the same way
/// [`require_nested_family`] already closes it for every two-level nested resource: after
/// resolution, the URL's path is required to match the schedule family shape below, or the call
/// is rejected before any request is sent. The shape is checked by [`require_family_path`] on the
/// URL with its trailing `/` removed.
///
/// # Errors
///
/// Returns [`Error::validation`] with `field: "schedule"` if `url` carries a query or fragment,
/// or its path does not match `networks/{id}/profiles/{id}/schedules/{id}`.
pub(crate) fn require_schedule_family(url: &Url) -> Result<Url, Error> {
    require_family_path(
        url.as_str().trim_end_matches('/'),
        SCHEDULE_FAMILY,
        "",
        "schedule",
    )?;
    Ok(url.clone())
}

#[cfg(test)]
mod tests {
    use super::{
        CADENCE_VALUES, require_hour, require_non_empty_string, require_schedule_family,
        resolve_nested_url, resolve_network_url, validate_cadence,
    };
    use crate::error::Error;
    use crate::links::resource_url;
    use crate::routes::ApiVersion;
    use serde_json::json;
    use url::Url;

    fn host() -> Url {
        Url::parse("http://mock.test:1234").unwrap()
    }

    // ===================== TestSharedSetterValidators / TestRequireHour =====================

    /// Asserts that `result` is a client-side validation error with exactly `field` and
    /// `message` (no envelope, no error code) and Python's `str()` rendering.
    fn assert_validation(result: Result<(), Error>, field: &str, message: &str) {
        match result {
            Err(Error::Validation {
                field: got_field,
                message: got_message,
                envelope: None,
                error_code: None,
            }) => {
                assert_eq!(got_field, field);
                assert_eq!(got_message, message);
            }
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn require_non_empty_string_rejects_empty_without_echoing() {
        let err = require_non_empty_string("field", "").unwrap_err();
        assert!(matches!(
            &err,
            Error::Validation { field, message, envelope: None, error_code: None }
                if field == "field" && message == "must be a non-empty string"
        ));
        assert_eq!(
            err.to_string(),
            "Validation error for 'field': must be a non-empty string"
        );
    }

    #[test]
    fn require_non_empty_string_accepts_any_non_empty_string() {
        // Not trimmed: whitespace-only is accepted, as in Python.
        for value in ["x", "None", "False", "0", "  padded  ", " ", "\t", "\u{e9}"] {
            assert!(
                require_non_empty_string("field", value).is_ok(),
                "{value:?}"
            );
        }
    }

    #[test]
    fn require_non_empty_string_reports_the_given_field_name() {
        assert_validation(
            require_non_empty_string("ssid", ""),
            "ssid",
            "must be a non-empty string",
        );
    }

    #[test]
    fn require_hour_accepts_hours_of_day() {
        for value in 0..=23 {
            assert!(require_hour("hour", value).is_ok(), "{value}");
        }
    }

    #[test]
    fn require_hour_rejects_out_of_range() {
        for value in [-1, 24, 99, i32::MIN, i32::MAX] {
            assert_validation(
                require_hour("hour", value),
                "hour",
                "must be an int from 0 to 23",
            );
        }
    }

    #[test]
    fn require_hour_reports_the_given_field_name() {
        assert_validation(
            require_hour("when", 99),
            "when",
            "must be an int from 0 to 23",
        );
        assert_eq!(
            require_hour("hour", -1).unwrap_err().to_string(),
            "Validation error for 'hour': must be an int from 0 to 23"
        );
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
        // The plain-path rule inside `resource_url` rejects the query string before
        // `require_nested_family` ever runs, so the error names the caller's argument
        // (`id_or_url`) rather than `child`.
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
        assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
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

    // ===================== TestResolveNetworkUrlConfinement =====================

    /// Resolves `network` with no parent and returns the URL as text.
    fn network_url(network: &str) -> String {
        resolve_network_url(&host(), network, None, ApiVersion::V2_2)
            .unwrap_or_else(|err| panic!("{network:?} must resolve: {err:?}"))
            .to_string()
    }

    /// Resolves a nested child with the given `prefix` and `suffix` and returns the URL as text.
    fn nested_url(network: &str, child: &str, prefix: &str, suffix: &str) -> Result<String, Error> {
        resolve_nested_url(
            &host(),
            network,
            child,
            prefix,
            suffix,
            None,
            None,
            ApiVersion::V2_2,
        )
        .map(|url| url.to_string())
    }

    #[test]
    fn resolve_network_url_accepts_an_id_a_path_and_a_url_naming_one_network() {
        assert_eq!(
            network_url("network_123"),
            "http://mock.test:1234/2.2/networks/network_123"
        );
        assert_eq!(
            network_url("/2.3/networks/network_123"),
            "http://mock.test:1234/2.3/networks/network_123"
        );
        assert_eq!(
            network_url("http://mock.test:1234/2.3/networks/network_123"),
            "http://mock.test:1234/2.3/networks/network_123"
        );
    }

    #[test]
    fn resolve_network_url_rejects_a_path_or_url_that_is_not_one_network() {
        for network in [
            "/2.2/account",
            "/2.2/eeros/9",
            "/2.2/networks/network_123/devices",
            "/2.2/networks/network_123?x=1",
            "/2.2/networks/network_123/..",
            "/2.2/networks/network_123\n",
            "http://mock.test:1234/2.2/account",
        ] {
            let err = resolve_network_url(&host(), network, None, ApiVersion::V2_2).unwrap_err();
            assert!(matches!(err, Error::Validation { .. }), "{network:?}");
        }
    }

    #[test]
    fn resolve_network_url_family_message_and_field() {
        let err = resolve_network_url(&host(), "/2.2/account", None, ApiVersion::V2_2).unwrap_err();
        assert!(matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "id_or_url"
                    && message == "must be a path under networks/{id} on the API host"
        ));
    }

    #[test]
    fn resolve_network_url_does_not_confine_the_parents_published_url() {
        let parent = json!({"url": "/2.3/networks/network_123"});
        let url =
            resolve_network_url(&host(), "/2.2/account", Some(&parent), ApiVersion::V2_2).unwrap();
        assert_eq!(
            url.as_str(),
            "http://mock.test:1234/2.3/networks/network_123"
        );
        // ...but the published url still gets the plain-path rule.
        let parent = json!({"url": "/2.3/networks/../account"});
        let err = resolve_network_url(&host(), "100", Some(&parent), ApiVersion::V2_2).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "link"));
    }

    // ===================== TestResolveNestedUrlChildValidation =====================

    #[test]
    fn resolve_nested_url_bare_child_that_is_not_a_single_segment_is_rejected() {
        for child in ["../account", "device?x=1", "a/b", "id#frag", "", " "] {
            let err = nested_url("network_123", child, "profiles", "/schedules").unwrap_err();
            assert!(matches!(err, Error::Validation { .. }), "{child:?}");
        }
    }

    #[test]
    fn resolve_nested_url_valid_bare_child_is_templated_with_suffix() {
        assert_eq!(
            nested_url("network_123", "profile_1", "profiles", "/schedules").unwrap(),
            "http://mock.test:1234/2.2/networks/network_123/profiles/profile_1/schedules"
        );
    }

    #[test]
    fn resolve_nested_url_path_child_bypasses_the_template() {
        assert_eq!(
            nested_url(
                "network_123",
                "/2.3/networks/network_123/profiles/profile_1",
                "profiles",
                "/schedules"
            )
            .unwrap(),
            "http://mock.test:1234/2.3/networks/network_123/profiles/profile_1/schedules"
        );
    }

    #[test]
    fn resolve_nested_url_path_network_confines_a_bare_child_too() {
        let err = nested_url("/2.2/account", "p1", "profiles", "").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
    }

    // ===================== TestResolveNestedUrlPathChildFamily =====================

    #[test]
    fn resolve_nested_url_path_outside_the_family_is_rejected() {
        for child in [
            "/2.2/account",
            "/2.2/networks/network_123/forwards/f_1",
            "/2.2/networks/other_net/profiles/profile_1",
            "/2.2/networks/network_123/profiles/profile_1?x=1",
            "/2.2/networks/network_123/profiles/profile_1#frag",
            "/2.2/networks/network_123/profiles/profile_1/extra",
            "http://mock.test:1234/2.2/networks/network_123/profiles",
        ] {
            let err = nested_url("network_123", child, "profiles", "/schedules").unwrap_err();
            assert!(matches!(err, Error::Validation { .. }), "{child:?}");
        }
    }

    #[test]
    fn resolve_nested_url_dot_segments_and_control_characters_are_rejected() {
        for child in [
            "/2.2/networks/123/profiles/..",
            "/2.2/networks/123/profiles/.",
            "/2.2/networks/123/profiles/../profile_1",
            "/2.2/networks/123/profiles/profile_1/../..",
            "/2.2/networks/../profiles/profile_1",
            "/2.2/networks/123/profiles/%2e%2e",
            "/2.2/networks/123/profiles/%2E",
            "/2.2/networks/123/profiles/profile_1\n",
            "/2.2/networks/123/profiles/profile\t_1",
            "/2.2/networks/123/profiles/profile_1 ",
            "/2.2/networks/123/profiles/profile\u{0}_1",
            "/2.2/networks/123/profiles/x/../p1",
            "http://mock.test:1234/2.2/networks/123/profiles/..",
            "http://mock.test:1234/2.2/networks/123/profiles/profile_1\r\n",
        ] {
            for suffix in ["", "/schedules"] {
                let err = nested_url("123", child, "profiles", suffix).unwrap_err();
                assert!(
                    matches!(err, Error::Validation { .. }),
                    "{child:?} {suffix:?}"
                );
            }
        }
    }

    #[test]
    fn resolve_nested_url_dot_children_report_the_callers_argument_and_the_dot_message() {
        for child in [
            "/2.2/networks/123/profiles/..",
            "/2.2/networks/123/profiles/.",
        ] {
            let err = nested_url("123", child, "profiles", "").unwrap_err();
            assert!(matches!(
                err,
                Error::Validation { field, message, .. }
                    if field == "id_or_url" && message == "must not contain dot segments"
            ));
        }
        let err = nested_url(
            "123",
            "http://mock.test:1234/2.2/networks/123/profiles/..",
            "profiles",
            "",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "url" && message == "must not contain dot segments"
        ));
    }

    #[test]
    fn resolve_nested_url_family_message_gains_the_trailing_id() {
        let err = nested_url("100", "/2.2/account", "profiles", "").unwrap_err();
        assert!(matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "child"
                    && message == "must be a path under networks/{id}/profiles/{id} on the API host"
        ));
    }

    #[test]
    fn resolve_nested_url_every_id_must_start_with_an_ascii_alphanumeric() {
        for child in [
            "/2.2/networks/100/profiles/_p1",
            "/2.2/networks/100/profiles/-p1",
            "/2.2/networks/100/profiles/.p1",
            "/2.2/networks/_100/profiles/p1",
        ] {
            let err = nested_url("100", child, "profiles", "").unwrap_err();
            assert!(
                matches!(&err, Error::Validation { field, .. } if field == "child"),
                "{child:?}: {err:?}"
            );
        }
    }

    #[test]
    fn resolve_nested_url_tolerates_any_number_of_trailing_slashes_on_the_child() {
        for suffix in ["", "/schedules"] {
            for slashes in ["", "/", "//", "///"] {
                assert_eq!(
                    nested_url(
                        "123",
                        &format!("/2.2/networks/123/profiles/p1{slashes}"),
                        "profiles",
                        suffix
                    )
                    .unwrap(),
                    format!("http://mock.test:1234/2.2/networks/123/profiles/p1{suffix}"),
                    "{slashes:?} {suffix:?}"
                );
                assert_eq!(
                    nested_url(
                        "123",
                        &format!("http://mock.test:1234/2.2/networks/123/profiles/p1{slashes}"),
                        "profiles",
                        suffix
                    )
                    .unwrap(),
                    format!("http://mock.test:1234/2.2/networks/123/profiles/p1{suffix}"),
                    "{slashes:?} {suffix:?}"
                );
            }
        }
    }

    #[test]
    fn resolve_nested_url_nested_prefix_with_several_segments_is_matched_literally() {
        let url = nested_url(
            "network_123",
            "/2.2/networks/network_123/insights/devices/aa:bb:cc:11:22:33",
            "insights/devices",
            "",
        )
        .unwrap();
        assert!(url.ends_with("/networks/network_123/insights/devices/aa:bb:cc:11:22:33"));
        let err = nested_url(
            "network_123",
            "/insights/profiles/p_1",
            "insights/devices",
            "",
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
        let err = nested_url(
            "network_123",
            "/2.2/networks/network_123/insights/profiles/p_1",
            "insights/devices",
            "",
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    #[test]
    fn resolve_nested_url_path_network_and_path_child_must_agree() {
        let err = nested_url(
            "/2.2/networks/network_123",
            "/2.2/networks/other_net/profiles/p",
            "profiles",
            "",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "child" && message == "must belong to the addressed network"
        ));
        assert_eq!(
            nested_url(
                "/2.2/networks/network_123",
                "/2.2/networks/network_123/profiles/p",
                "profiles",
                ""
            )
            .unwrap(),
            "http://mock.test:1234/2.2/networks/network_123/profiles/p"
        );
    }

    #[test]
    fn resolve_nested_url_child_is_resolved_like_resource_url_with_an_id_only_template() {
        // The nested path goes through `resource_url` with a family-less template, so the
        // plain-path rule is exactly resource_url's.
        let direct = resource_url(
            &host(),
            "/2.2/networks/123/profiles/p1///",
            "{id}/schedules",
            ApiVersion::V2_2,
        )
        .unwrap();
        assert_eq!(
            direct.as_str(),
            "http://mock.test:1234/2.2/networks/123/profiles/p1/schedules"
        );
    }

    // ===================== require_schedule_family (security finding 2) =====================

    #[test]
    fn require_schedule_family_accepts_a_genuine_schedule_path() {
        let url =
            Url::parse("http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/s1").unwrap();
        assert_eq!(require_schedule_family(&url).unwrap(), url);
    }

    #[test]
    fn require_schedule_family_rejects_a_bare_networks_collection_path() {
        let url = Url::parse("http://mock.test:1234/2.2/networks").unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));
    }

    #[test]
    fn require_schedule_family_rejects_an_unrelated_network_path() {
        let url = Url::parse("http://mock.test:1234/2.2/networks/1").unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));
    }

    #[test]
    fn require_schedule_family_rejects_a_query_or_fragment() {
        let url = Url::parse("http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/s1?x=1")
            .unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));

        let url =
            Url::parse("http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/s1#frag")
                .unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));
    }

    #[test]
    fn require_schedule_family_rejects_trailing_extra_segments() {
        let url =
            Url::parse("http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/s1/extra")
                .unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));
    }

    #[test]
    fn require_schedule_family_rejects_dot_segments_and_non_alphanumeric_leading_ids() {
        for raw in [
            "http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/s1/%2e%2e",
            "http://mock.test:1234/2.2/networks/100/profiles/p1/schedules/_s1",
        ] {
            let url = Url::parse(raw).unwrap();
            let err = require_schedule_family(&url).unwrap_err();
            assert!(
                matches!(&err, Error::Validation { field, .. } if field == "schedule"),
                "{raw}"
            );
        }
    }

    #[test]
    fn require_schedule_family_reports_the_family_message() {
        let url = Url::parse("http://mock.test:1234/2.2/networks/1").unwrap();
        let err = require_schedule_family(&url).unwrap_err();
        assert!(matches!(
            err,
            Error::Validation { message, .. }
                if message
                    == "must be a path under networks/{id}/profiles/{id}/schedules/{id} on the API host"
        ));
    }
}
