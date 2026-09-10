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
/// # Examples
///
/// ```
/// use rusteero::util::id_from_url;
///
/// assert_eq!(id_from_url("12345").unwrap(), "12345");
/// assert_eq!(id_from_url("/2.2/networks/12345/").unwrap(), "12345");
/// assert!(id_from_url("").is_err());
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
}
