//! Security regression tests for the path-traversal / empty-segment finding fixed in
//! `src/routes.rs` (`validate_segment`) and `src/transport.rs` (`Transport::render_url`).
//!
//! `url` 2.5.8's path-segment parser strips ASCII tab/CR/LF from a segment *before*
//! percent-encoding and dot-segment removal run, so a placeholder value that is not literally
//! `".."` (e.g. `"..\n"`) could previously be turned into `".."` by the time it reached the
//! encoder, collapsing a destructive verb (`DELETE`, or a `/2.3` `PUT`) onto the parent
//! collection instead of the one item the caller named. An empty substituted value had the same
//! effect. See `src/routes.rs`'s module docs ("Path traversal") for the full mechanism.
//!
//! Every test below proves the fix at the network boundary: a hostile value must be rejected by
//! `Transport::render_url` (surfaced here as an `Err(Error::Validation { .. })` from the
//! endpoint method) *before* any HTTP request is sent — a catch-all `wiremock::matchers::any()`
//! mock with `.expect(0)` fails loudly if a hostile value ever escapes to the wire. Per
//! the crate's testing conventions, this is covered for at least one route per shape: a DELETE-by-id
//! (`BlacklistApi::remove_from_blacklist`), a PUT-by-id on `/2.3` (`DevicesApi::pause_device`),
//! and a GET-by-id (`EerosApi::get_eero`) — proving the fix on the destructive verbs
//! specifically, not just reads.
//!
//! A second table of already-legitimate or already-safe values is asserted to still work
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
/// Each one either contains a byte `url` 2.5.8 strips before its dot-segment check runs
/// (`"..\n"`, `".\t."`, `"\t.."`, `"..\t"`, `"..\r\n"`, `"\u{0}"`, `"a\u{7f}b"`), is itself a
/// literal dot segment with no stripping involved (`".."`, `"."`), or is empty (`""`).
const HOSTILE_VALUES: &[&str] = &[
    "..\n", ".\t.", "\t..", "..\t", "..\r\n", "..", ".", "", "\u{0}", "a\u{7f}b",
];

/// Every value this suite asserts is left unaffected by the fix: legitimate identifier shapes,
/// plus values that are already safely opaque once percent-encoded and never reach the `url`
/// crate's dot-segment logic at all.
const SAFE_VALUES: &[&str] = &[
    "device-0001",
    "aa:bb:cc:00:00:01",
    "a/b",
    "%2e%2e",
    "..%2f..",
    "．．", // fullwidth dots (U+FF0E) — not ASCII '.', never trips dot-segment removal
    "a?b",
    "a#b",
];

/// Mounts a catch-all mock on `mock` that must never be hit, matching the "no request may be
/// sent for a rejected value" half of every test below.
async fn deny_all_requests(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;
}

/// Asserts `err` is `Error::Validation` for exactly `field`, and not some other error variant a
/// regression could otherwise disguise itself as.
fn assert_validation_error_for_field(err: &Error, field: &str) {
    match err {
        Error::Validation { field: got, .. } => {
            assert_eq!(got, field, "wrong field name in rejection for {err:?}");
        }
        other => panic!("expected Error::Validation for field {field:?}, got {other:?}"),
    }
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
            .remove_from_blacklist("network-0001", value)
            .await
            .expect_err(&format!(
                "{value:?} must be rejected, not sent to the server"
            ));
        assert_validation_error_for_field(&err, "mac_or_device_id");
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
        api.remove_from_blacklist("network-0001", value)
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
        assert_validation_error_for_field(&err, "device_id");
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
        let err = api.get_eero(value).await.expect_err(&format!(
            "{value:?} must be rejected, not sent to the server"
        ));
        assert_validation_error_for_field(&err, "eero_id");
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
        api.get_eero(value)
            .await
            .unwrap_or_else(|err| panic!("{value:?} must still be accepted, got {err:?}"));
    }
    Ok(())
}
