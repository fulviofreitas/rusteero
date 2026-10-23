//! Security regression tests for the path-traversal / empty-segment finding, proven at v8.0.4
//! through real endpoint calls rather than the pre-v8.0.4 `Route`/`validate_segment` model this
//! file used to pin directly (that model is gone — see `src/routes/mod.rs`'s module docs).
//!
//! Every identifier substituted into a resolved URL at v8.0.4 goes through
//! [`rusteero::links::validate_identifier`] (a strict `^[A-Za-z0-9][A-Za-z0-9._:-]*$` whitelist,
//! plus an explicit `".."` rejection) — either directly (`child_url`/`resource_url`) or, for
//! `DevicesApi::pause_device`, after [`rusteero::util::id_from_url`] first extracts a trailing
//! path segment from a path/URL-shaped value. This is *stricter* than the pre-v8.0.4 model (which
//! percent-encoded almost anything into a single opaque segment and only denylisted a known-bad
//! shape): a value such as `"a/b"` or `"a?b"` was previously accepted-and-encoded, and is now
//! rejected outright by [`EerosApi::get_eero`]/[`BlacklistApi::remove_from_blacklist`] (ported
//! faithfully from `eero-api`'s own `_IDENTIFIER_RE` at v8.0.4 — see
//! `git -C eero-api show v8.0.4:src/eero/api/links.py`). `DevicesApi::pause_device` is the one
//! exception: it normalises `mac` through `id_from_url` first, which extracts the segment after
//! the last `/` rather than rejecting a slash outright, matching `_update_device`'s own
//! `id_from_url(mac)` call (`devices.py:44-65`) before the same identifier check runs.
//!
//! Every test below proves the fix at the network boundary: a hostile value must be rejected as
//! `Error::Validation` *before* any HTTP request is sent — a catch-all `wiremock::matchers::any()`
//! mock with `.expect(0)` fails loudly if a hostile value ever escapes to the wire. This is
//! covered for at least one route per shape: a DELETE-by-id (`BlacklistApi::remove_from_blacklist`),
//! a PUT-by-id on `/2.3` (`DevicesApi::pause_device`), and a GET-by-id (`EerosApi::get_eero`) —
//! proving the fix on the destructive verbs specifically, not just reads.
//!
//! The exact `field` name on the returned `Error::Validation` is deliberately **not** pinned here:
//! it depends on which of `id_from_url`/`resource_url`/`child_url` first rejects the value (`"id"`,
//! `"id_or_url"`, or `"child"` depending on the route and the value's shape) — an implementation
//! detail, not part of the security guarantee this suite exists to pin (fail closed, zero requests
//! sent).
//!
//! A second table of legitimate, unambiguously-safe identifiers is asserted to still work
//! *unchanged* across the same three shapes, so this fix is proven not to have introduced a
//! false-positive rejection alongside the true-positive one.

mod common;

use std::sync::Arc;

use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::endpoints::devices::DevicesApi;
use rusteero::endpoints::eeros::EerosApi;
use rusteero::error::Error;
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN};

/// Every value this suite asserts is REJECTED before any request reaches the server.
///
/// Each one either contains a byte outside `links::validate_identifier`'s whitelist
/// (`[A-Za-z0-9._:-]`), is itself a literal dot segment (`".."`, `"."`), or is empty (`""`).
const HOSTILE_VALUES: &[&str] = &[
    "..\n", ".\t.", "\t..", "..\t", "..\r\n", "..", ".", "", "\u{0}", "a\u{7f}b",
];

/// Every value this suite asserts is left unaffected by the fix: legitimate identifier shapes —
/// alphanumeric, optionally with `.`/`_`/`:`/`-` — that were valid before this fix and remain
/// valid under `links::validate_identifier`'s stricter whitelist.
///
/// Unlike the pre-v8.0.4 suite this file replaces, this list deliberately excludes values that
/// were merely "safe because percent-encoded" under the old, more permissive `Route`/
/// `validate_segment` model (slashes, `%`, `?`, `#`, non-ASCII look-alike dots): those are no
/// longer valid single-segment identifiers at v8.0.4 and are correctly rejected now — ported
/// faithfully from `eero-api`'s own identifier regex, not a regression.
const SAFE_VALUES: &[&str] = &["device-0001", "aa:bb:cc:00:00:01", "abc123", "a.b_c-d:e"];

/// Mounts a catch-all mock on `mock` that must never be hit, matching the "no request may be
/// sent for a rejected value" half of every test below.
async fn deny_all_requests(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;
}

/// Asserts `err` is `Error::Validation` — fail-closed, not some other error variant a regression
/// could otherwise disguise itself as. See this file's module docs for why the `field` name
/// itself is deliberately not pinned.
fn assert_is_validation_error(err: &Error) {
    assert!(
        matches!(err, Error::Validation { .. }),
        "expected Error::Validation, got {err:?}"
    );
}

// ===================== DELETE-by-id: BlacklistApi::remove_from_blacklist =====================

#[tokio::test]
async fn remove_from_blacklist_rejects_every_hostile_value_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in HOSTILE_VALUES {
        let err = api
            .remove_from_blacklist("network-0001", value, None)
            .await
            .expect_err(&format!(
                "{value:?} must be rejected, not sent to the server"
            ));
        assert_is_validation_error(&err);
    }
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_still_accepts_every_safe_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(SAFE_VALUES.len() as u64)
        .mount(&mock.server)
        .await;
    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in SAFE_VALUES {
        api.remove_from_blacklist("network-0001", value, None)
            .await
            .unwrap_or_else(|err| panic!("{value:?} must still be accepted, got {err:?}"));
    }
    Ok(())
}

// ===================== PUT-by-id on /2.3: DevicesApi::pause_device =====================

#[tokio::test]
async fn pause_device_rejects_every_hostile_value_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in HOSTILE_VALUES {
        let err = api
            .pause_device("network-0001", value, true)
            .await
            .expect_err(&format!(
                "{value:?} must be rejected, not sent to the server"
            ));
        assert_is_validation_error(&err);
    }
    Ok(())
}

#[tokio::test]
async fn pause_device_still_accepts_every_safe_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(SAFE_VALUES.len() as u64)
        .mount(&mock.server)
        .await;
    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in SAFE_VALUES {
        api.pause_device("network-0001", value, true)
            .await
            .unwrap_or_else(|err| panic!("{value:?} must still be accepted, got {err:?}"));
    }
    Ok(())
}

// ===================== GET-by-id: EerosApi::get_eero =====================

#[tokio::test]
async fn get_eero_rejects_every_hostile_value_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in HOSTILE_VALUES {
        let err = api.get_eero(value, None).await.expect_err(&format!(
            "{value:?} must be rejected, not sent to the server"
        ));
        assert_is_validation_error(&err);
    }
    Ok(())
}

#[tokio::test]
async fn get_eero_still_accepts_every_safe_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(SAFE_VALUES.len() as u64)
        .mount(&mock.server)
        .await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    for value in SAFE_VALUES {
        api.get_eero(value, None)
            .await
            .unwrap_or_else(|err| panic!("{value:?} must still be accepted, got {err:?}"));
    }
    Ok(())
}

// ===================== absolute URLs: the origin rule at the endpoint boundary =====================

/// The mock server's `host:port`, as it appears in an absolute URL.
fn mock_authority(mock: &MockEero) -> String {
    mock.uri().trim_start_matches("http://").to_owned()
}

/// The mock server's port.
fn mock_port(mock: &MockEero) -> u16 {
    mock_authority(mock)
        .rsplit(':')
        .next()
        .and_then(|port| port.parse().ok())
        .expect("a MockServer URI ends in its port")
}

/// The one message the origin rule gives, for the configured host `127.0.0.1`. It is a fixed
/// string, so a message that echoed any part of the offending URL could not equal it.
const ORIGIN_MESSAGE: &str =
    "must be an absolute http URL on 127.0.0.1 with no userinfo and no other port";

/// Asserts `err` is the origin-rule validation error: field `url` and exactly [`ORIGIN_MESSAGE`].
fn assert_is_origin_error(err: &Error) {
    match err {
        Error::Validation { field, message, .. } => {
            assert_eq!(field, "url");
            assert_eq!(message, ORIGIN_MESSAGE);
        }
        other => panic!("expected Error::Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn get_eero_rejects_userinfo_on_the_configured_host_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let authority = mock_authority(&mock);

    for userinfo in ["user:pass@", "user@", ":pass@", "@"] {
        let url = format!("http://{userinfo}{authority}/2.2/eeros/1");
        let err = api
            .get_eero(&url, None)
            .await
            .expect_err(&format!("{url:?} must be rejected, not sent to the server"));
        assert_is_origin_error(&err);
    }
    Ok(())
}

#[tokio::test]
async fn get_eero_rejects_another_port_on_the_configured_host_before_any_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let port = mock_port(&mock);
    let other = port.checked_add(1).unwrap_or(1);

    for authority in [
        format!("127.0.0.1:{other}"),
        "127.0.0.1".to_owned(),
        "127.0.0.1:0".to_owned(),
    ] {
        let url = format!("http://{authority}/2.2/eeros/1");
        let err = api
            .get_eero(&url, None)
            .await
            .expect_err(&format!("{url:?} must be rejected, not sent to the server"));
        assert_is_origin_error(&err);
    }
    Ok(())
}

#[tokio::test]
async fn get_eero_rejects_unparseable_and_rewritten_authorities_before_any_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let authority = mock_authority(&mock);
    let port = mock_port(&mock);

    for url in [
        "http://127.0.0.1:abc/2.2/eeros/1".to_owned(),
        "http://127.0.0.1:99999/2.2/eeros/1".to_owned(),
        "http://[::1/2.2/eeros/1".to_owned(),
        "http://127.0.0.1\u{ff1a}99/2.2/eeros/1".to_owned(),
        format!("http://evil.example\\@{authority}/2.2/eeros/1"),
        format!("http://{authority}\\@evil.example/2.2/eeros/1"),
        format!("http://{authority}\u{ff0f}@evil.example/2.2/eeros/1"),
        format!("http://%31{}/2.2/eeros/1", &authority[1..]),
        // The parser folds IPv4 shorthand into `127.0.0.1`, port included, so only the raw
        // authority rule can tell this from the configured origin.
        format!("http://127.1:{port}/2.2/eeros/1"),
    ] {
        let err = api
            .get_eero(&url, None)
            .await
            .expect_err(&format!("{url:?} must be rejected, not sent to the server"));
        assert_is_origin_error(&err);
    }
    Ok(())
}

/// The shape rule runs before the origin rule: an off-origin URL carrying a query is reported
/// for its query, not its origin, and no request is sent either way.
#[tokio::test]
async fn get_eero_reports_a_bad_shape_before_a_bad_origin() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    deny_all_requests(&mock).await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));

    let err = api
        .get_eero("https://evil.example/2.2/eeros/1?x=1", None)
        .await
        .expect_err("a query string must be rejected");
    assert!(
        matches!(
            &err,
            Error::Validation { field, message, .. }
                if field == "url" && message == "must not carry a query or fragment"
        ),
        "unexpected error: {err:?}"
    );
    Ok(())
}

#[tokio::test]
async fn get_eero_accepts_an_absolute_url_on_the_configured_origin() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/2.2/eeros/e_1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(2)
        .mount(&mock.server)
        .await;
    let api = EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let uri = mock.uri();

    api.get_eero(&format!("{uri}/2.2/eeros/e_1"), None).await?;
    // An upper-case scheme and host are the same origin, and one trailing slash is tolerated.
    let shouty = format!("{uri}/2.2/eeros/e_1/").replacen("http", "HTTP", 1);
    api.get_eero(&shouty, None).await?;
    Ok(())
}
