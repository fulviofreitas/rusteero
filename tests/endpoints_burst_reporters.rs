//! HTTP integration tests for `BurstReportersApi` (`src/endpoints/burst_reporters.rs`) against a
//! local `wiremock` server per the crate's testing conventions.
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

use common::{MockEero, TEST_TOKEN, session_cookie};

/// Builds a [`BurstReportersApi`] pointed at `mock`, wrapping a `Transport` already
/// authenticated with [`TEST_TOKEN`].
fn burst_reporters_api(mock: &MockEero) -> BurstReportersApi {
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_burst_reporters =====================

#[tokio::test]
async fn get_burst_reporters_hits_burst_reporters_path_and_returns_the_served_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": [{"id": "reporter-0001", "type": "burst"}],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api.get_burst_reporters("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
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
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api.create_burst_reporter("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
