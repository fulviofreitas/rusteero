//! Small helpers shared across the crate.

use crate::error::Error;

/// Extracts the trailing ID segment from a bare ID or an Eero API URL / URL fragment.
///
/// Ported from `id_from_url` (`eero-api` `src/eero/api/base.py:36-57`). Accepts either a bare
/// identifier (e.g. `"12345"`, `"p_abc"`) or a URL / URL fragment as returned by the API (e.g.
/// `"/2.2/networks/12345"`, `"/2.2/networks/12345/profiles/p_67890"`, or a fully-qualified
/// `"https://api-user.e2ro.com/2.2/networks/12345"`). Any trailing slashes are stripped before
/// the last path segment is extracted; in every case the trailing segment is returned unchanged.
///
/// # Errors
///
/// Returns [`Error::Validation`] with `field: "id_or_url"` when `id_or_url` is empty, matching
/// Python's `EeroValidationException("id_or_url", "must be a non-empty string")`
/// (`base.py:53-54`), whose formatted message is
/// `"Validation error for 'id_or_url': must be a non-empty string"`.
///
/// Python's guard also rejects `None` and non-`str` inputs
/// (`tests/api/test_id_from_url.py::test_none_raises_validation_exception`,
/// `::test_non_string_int_raises_validation_exception`); those cases have no Rust equivalent
/// since `id_or_url: &str` is statically guaranteed to already be a string slice, so only the
/// empty-string case from that Python test module is reachable and ported here.
///
/// Also returns [`Error::Validation`] with `field: "id_or_url"` when the *extracted* trailing
/// segment is empty (e.g. `id_from_url("/")`, or any input consisting entirely of slashes). This
/// is a deliberate departure from parity, called out separately from the case above: Python's
/// `id_from_url` has no such guard and returns `""` for the same input. A caller that feeds an
/// empty id straight into a wire route (see `routes::validate_segment`) would otherwise
/// collapse a per-item route onto its parent collection while keeping the same, possibly
/// destructive, HTTP verb — closing the gap here, at the one place every such id is derived,
/// is more robust than relying on every call site downstream to notice.
///
/// # Examples
///
/// ```
/// use rusteero::util::id_from_url;
///
/// assert_eq!(id_from_url("12345").unwrap(), "12345");
/// assert_eq!(id_from_url("/2.2/networks/12345/").unwrap(), "12345");
/// assert!(id_from_url("").is_err());
/// assert!(id_from_url("/").is_err());
/// ```
pub fn id_from_url(id_or_url: &str) -> Result<String, Error> {
    if id_or_url.is_empty() {
        return Err(Error::Validation {
            field: "id_or_url".to_string(),
            message: "must be a non-empty string".to_string(),
        });
    }
    // Python: `id_or_url.rstrip("/")` strips every trailing slash, not just one.
    let stripped = id_or_url.trim_end_matches('/');
    // Python: `stripped.rsplit("/", 1)[-1]` — the segment right of the last slash, or the whole
    // (already-stripped) string when there is no slash at all.
    let segment = stripped.rsplit('/').next().unwrap_or(stripped);
    if segment.is_empty() {
        // Reachable only when `id_or_url` is composed entirely of slashes (e.g. "/", "///"):
        // `trim_end_matches` then leaves `stripped` empty, and splitting an empty string still
        // yields one (empty) segment. Rejecting this outright, rather than returning `Ok("")`
        // as Python does, is the fix for the empty-identifier finding — see this function's
        // doc comment.
        return Err(Error::Validation {
            field: "id_or_url".to_string(),
            message: "resolves to an empty path segment".to_string(),
        });
    }
    Ok(segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::id_from_url;
    use crate::error::Error;

    // ===================== Happy paths (test_id_from_url.py::TestIdFromUrlHappyPaths) =====================

    #[test]
    fn bare_numeric_id_returned_unchanged() {
        assert_eq!(id_from_url("12345").unwrap(), "12345");
    }

    #[test]
    fn bare_opaque_id_returned_unchanged() {
        assert_eq!(id_from_url("p_abc123").unwrap(), "p_abc123");
    }

    #[test]
    fn full_url_fragment_extracts_trailing_id() {
        assert_eq!(id_from_url("/2.2/networks/12345").unwrap(), "12345");
    }

    #[test]
    fn nested_url_fragment_extracts_trailing_id() {
        assert_eq!(
            id_from_url("/2.2/networks/12345/profiles/p_67890").unwrap(),
            "p_67890"
        );
    }

    #[test]
    fn url_with_trailing_slash_handled() {
        assert_eq!(id_from_url("/2.2/networks/12345/").unwrap(), "12345");
    }

    #[test]
    fn absolute_url_extracts_trailing_id() {
        assert_eq!(
            id_from_url("https://api-user.e2ro.com/2.2/networks/12345").unwrap(),
            "12345"
        );
    }

    // ===================== Validation errors (test_id_from_url.py::TestIdFromUrlValidationErrors) =====================
    //
    // Python also covers `None` and a non-string `int` input raising the same exception; both
    // are unreachable here because `id_or_url: &str` rules them out at compile time (see the
    // doc comment on `id_from_url`).

    #[test]
    fn empty_string_raises_validation_error() {
        let err = id_from_url("").unwrap_err();
        match err {
            Error::Validation { field, message } => {
                assert_eq!(field, "id_or_url");
                assert_eq!(message, "must be a non-empty string");
            }
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }

    // ===================== Empty-extracted-segment errors (security finding, no Python equivalent) =====================
    //
    // Python's `id_from_url` has no guard here at all and returns `""` for every input below;
    // rejecting an empty *extracted* segment is a deliberate Rust-side hardening documented on
    // `id_from_url` itself, not a parity regression.

    #[test]
    fn single_slash_raises_validation_error_for_empty_segment() {
        let err = id_from_url("/").unwrap_err();
        match err {
            Error::Validation { field, message } => {
                assert_eq!(field, "id_or_url");
                assert_eq!(message, "resolves to an empty path segment");
            }
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }

    #[test]
    fn all_slashes_raises_validation_error_for_empty_segment() {
        let err = id_from_url("///").unwrap_err();
        match err {
            Error::Validation { field, message } => {
                assert_eq!(field, "id_or_url");
                assert_eq!(message, "resolves to an empty path segment");
            }
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }
}
