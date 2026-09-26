//! HTTP integration tests for `BurstReportersApi` (`src/endpoints/burst_reporters.rs`,
//! POST-only since v8.0.0) against a local `wiremock` server per the crate's testing
//! conventions.
//!
//! `get_burst_reporters` was removed upstream in v8.0.0 (the endpoint 404s; the resource is
//! POST-only) and has no test here — see `.claude/tasks/briefs/v8/g7-backup-members.md` §2.
//!
//! `BurstReportersApi` has no committed fixture file, so every response body here is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_burst_reporters.py`) use — no real MACs, serials, IPs or names.
//!
//! `create_burst_reporter` pins the exact verb, path and JSON body via `body_json`, with an
//! `.expect(n)` call count verified at server-drop time.

mod common;

use std::sync::Arc;

use rusteero::endpoints::burst_reporters::BurstReportersApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds a [`BurstReportersApi`] pointed at `mock`, wrapping a `Transport` already
/// authenticated with [`TEST_TOKEN`].
fn burst_reporters_api(mock: &MockEero) -> BurstReportersApi {
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== create_burst_reporter =====================

#[tokio::test]
async fn create_burst_reporter_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst", "target": "192.0.2.30" });
    let response = json!({ "meta": { "code": 200 }, "data": { "id": "reporter-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", payload, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_prefers_a_parent_supplied_link_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst" });
    let response = json!({ "meta": { "code": 200 }, "data": { "id": "reporter-0002" } });

    // Only the linked (2.3) path is mocked — if `create_burst_reporter` fell back to the bare-id
    // template (2.2), this mock's `.expect(1)` would fail at server-drop time with zero calls
    // received.
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent =
        json!({"resources": {"burst_reporters": "/2.3/networks/network-0001/burst_reporters"}});
    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", payload, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_falls_back_to_the_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst" });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api
        .create_burst_reporter("network-0001", payload, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
