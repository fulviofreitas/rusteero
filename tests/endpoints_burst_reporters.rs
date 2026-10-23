//! HTTP integration tests for `BurstReportersApi` (`src/endpoints/burst_reporters.rs`,
//! POST-only since v8.0.0) against a local `wiremock` server per the crate's testing
//! conventions.
//!
//! `get_burst_reporters` was removed upstream in v8.0.0 (the endpoint 404s; the resource is
//! POST-only) and has no test here.
//!
//! `BurstReportersApi` has no committed fixture file, so every response body here is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_burst_reporters.py`) use — no real MACs, serials, IPs or names.
//!
//! `create_burst_reporter` pins the exact verb, path, content type and body (the `""` body by
//! default, the deprecated JSON passthrough otherwise) with an `.expect(n)` call count verified
//! at server-drop time, and pins every log line it emits with [`common::capture_logs`].

mod common;

use std::sync::Arc;

use rstest::rstest;
use rusteero::endpoints::burst_reporters::BurstReportersApi;
use rusteero::error::Error;
use serde_json::{Value, json};
use tracing::Level;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// The text Python's `DeprecationWarning` carries (`burst_reporters.py:90-96` at `v8.4.1`).
const DEPRECATION: &str = "The reporter_data parameter of create_burst_reporter is deprecated and \
     will be removed in 9.0.0; the API operation takes no body.";

/// The uncharacterised-write operation on the deprecated non-empty-body path.
const DEPRECATED_BODY_OPERATION: &str =
    "request live-usage burst with a deprecated body for network";

/// What every uncharacterised-write warning contains.
const UNCHARACTERISED: &str = "not been fully characterised";

/// Builds a [`BurstReportersApi`] pointed at `mock`, wrapping a `Transport` already
/// authenticated with [`TEST_TOKEN`].
fn burst_reporters_api(mock: &MockEero) -> BurstReportersApi {
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// The envelope the live API returned for a burst request.
fn next_burst_envelope() -> Value {
    json!({ "meta": { "code": 200 }, "data": { "next_burst": "2026-10-20T00:01:00Z" } })
}

/// Mounts `POST <post_path>` expecting the authenticated `""` body (`Content-Type:
/// application/json`) exactly `calls` times.
async fn mount_empty_body_post(mock: &MockEero, post_path: &str, calls: u64) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(next_burst_envelope()))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// The positive control for a negative log assertion: the capture still sees an
/// uncharacterised-write warning when one is emitted, so "none was logged" cannot pass
/// vacuously.
fn assert_capture_sees_warnings(logs: &common::LogCapture) {
    let before = logs.count(Level::WARN, UNCHARACTERISED);
    rusteero::links::warn_uncharacterised_write("probe");
    assert_eq!(logs.count(Level::WARN, UNCHARACTERISED), before + 1);
}

/// Whether the mock server has seen no request at all.
async fn received_nothing(mock: &MockEero) -> bool {
    mock.server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty()
}

// ===================== create_burst_reporter: the "" body =====================

#[tokio::test]
async fn create_burst_reporter_without_data_sends_the_empty_json_string_body() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", None, None)
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_prefers_a_parent_supplied_link_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // Only the linked (2.3) path is mocked for success — if `create_burst_reporter` fell back to
    // the bare-id template (2.2), the `.expect(0)` mock below would fail at server-drop time.
    mount_empty_body_post(&mock, "/2.3/networks/network-0001/burst_reporters", 1).await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent =
        json!({"resources": {"burst_reporters": "/2.3/networks/network-0001/burst_reporters"}});
    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", None, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_falls_back_to_the_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", None, None)
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = BurstReportersApi::new(Arc::new(mock.transport_anonymous()));

    let err = api
        .create_burst_reporter("network-0001", None, None)
        .await
        .expect_err("no session must fail before any request");

    assert!(err.is_auth_error());
    assert!(received_nothing(&mock).await);
    Ok(())
}

// ===================== create_burst_reporter: reporter_data (deprecated) =====================

#[tokio::test]
async fn create_burst_reporter_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst", "target": "192.0.2.30" });
    let response = json!({ "meta": { "code": 200 }, "data": { "id": "reporter-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", Some(payload), None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_deprecated_body_still_uses_the_parent_supplied_link()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst" });
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(next_burst_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent =
        json!({"resources": {"burst_reporters": "/2.3/networks/network-0001/burst_reporters"}});
    let api = burst_reporters_api(&mock);
    api.create_burst_reporter("network-0001", Some(payload), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_with_an_empty_object_sends_the_empty_json_string_body()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", Some(json!({})), None)
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    Ok(())
}

/// Python's falsy values (`None`/`null`, `False`, `0`, `0.0`, `""`, `[]`, `{}`) all select the
/// `""` body (`if reporter_data:`), never a JSON body, and never the uncharacterised-write
/// warning.
#[rstest]
#[case::null(json!(null))]
#[case::false_(json!(false))]
#[case::zero(json!(0))]
#[case::zero_float(json!(0.0))]
#[case::empty_string(json!(""))]
#[case::empty_array(json!([]))]
#[case::empty_object(json!({}))]
#[tokio::test]
async fn create_burst_reporter_with_a_falsy_value_sends_the_empty_json_string_body(
    #[case] falsy: Value,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.create_burst_reporter("network-0001", Some(falsy), None)
        .await?;

    assert_eq!(logs.count(Level::WARN, UNCHARACTERISED), 0);
    assert_capture_sees_warnings(&logs);
    Ok(())
}

/// Every other value is truthy and is forwarded unchanged as the JSON body.
#[rstest]
#[case::true_(json!(true))]
#[case::one(json!(1))]
#[case::text(json!("burst"))]
#[case::array(json!([1]))]
#[case::object(json!({"type": "burst"}))]
#[tokio::test]
async fn create_burst_reporter_with_a_truthy_value_sends_it_as_the_json_body(
    #[case] truthy: Value,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(truthy.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(next_burst_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    api.create_burst_reporter("network-0001", Some(truthy), None)
        .await?;
    Ok(())
}

// ===================== create_burst_reporter: log lines =====================

#[tokio::test]
async fn create_burst_reporter_empty_body_does_not_log_the_uncharacterised_write_warning()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    let env = api
        .create_burst_reporter("network-0001", None, None)
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    // Neither the v8.0.4 operation, nor the v8.4.0 one, nor the deprecation notice.
    assert_eq!(logs.count(Level::WARN, UNCHARACTERISED), 0);
    assert!(
        logs.warnings().is_empty(),
        "unexpected warnings: {:?}",
        logs.warnings()
    );
    assert_capture_sees_warnings(&logs);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_deprecated_body_still_logs_the_uncharacterised_write_warning()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({"type": "burst"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(next_burst_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.create_burst_reporter("network-0001", Some(json!({"type": "burst"})), None)
        .await?;

    assert_eq!(logs.count(Level::WARN, UNCHARACTERISED), 1);
    assert_eq!(
        logs.count(Level::WARN, DEPRECATED_BODY_OPERATION),
        1,
        "the warning must carry the deprecated-body operation: {:?}",
        logs.warnings()
    );
    // The v8.0.4 and v8.4.0 operation strings are gone.
    assert_eq!(
        logs.count(Level::WARN, "create burst reporter for network"),
        0
    );
    assert_eq!(
        logs.count(Level::WARN, "request live-usage burst for network"),
        0
    );
    // One deprecation notice, and nothing that names the caller-supplied network id.
    assert_eq!(logs.count(Level::WARN, DEPRECATION), 1);
    assert_eq!(logs.warnings().len(), 2, "{:?}", logs.warnings());
    assert!(
        logs.warnings()
            .iter()
            .all(|line| !line.contains("network-0001"))
    );
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_logs_the_deprecation_once_for_any_supplied_value()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // `Some({})` selects the `""` body but is still a supplied value, so it is still deprecated.
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.create_burst_reporter("network-0001", Some(json!({})), None)
        .await?;

    assert_eq!(logs.warnings(), vec![DEPRECATION.to_owned()]);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_treats_json_null_exactly_like_none() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_empty_body_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.create_burst_reporter("network-0001", Some(Value::Null), None)
        .await?;

    assert!(
        logs.warnings().is_empty(),
        "unexpected warnings: {:?}",
        logs.warnings()
    );
    assert_capture_sees_warnings(&logs);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_logs_the_deprecation_before_the_url_is_resolved()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // An off-host absolute URL fails URL resolution, so the truthy body never reaches the
    // uncharacterised-write warning (which follows resolution), but the deprecation notice —
    // logged first, as Python's `DeprecationWarning` is — has already fired.
    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    let err = api
        .create_burst_reporter(
            "https://attacker.invalid/2.2/networks/network-0001",
            Some(json!({"type": "burst"})),
            None,
        )
        .await
        .expect_err("an off-host network URL must be rejected before any request");

    assert!(matches!(err, Error::Validation { .. }), "{err:?}");
    assert!(received_nothing(&mock).await);
    assert_eq!(logs.count(Level::WARN, UNCHARACTERISED), 0);
    assert_eq!(logs.count(Level::WARN, DEPRECATION), 1);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_resolution_failure_without_data_logs_nothing() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;

    let api = burst_reporters_api(&mock);
    let (_guard, logs) = common::capture_logs();
    let err = api
        .create_burst_reporter(
            "https://attacker.invalid/2.2/networks/network-0001",
            None,
            None,
        )
        .await
        .expect_err("an off-host network URL must be rejected before any request");

    assert!(matches!(err, Error::Validation { .. }), "{err:?}");
    assert!(
        logs.warnings().is_empty(),
        "unexpected warnings: {:?}",
        logs.warnings()
    );
    assert!(received_nothing(&mock).await);
    Ok(())
}
