//! HTTP integration tests for `ForwardsApi` (`src/endpoints/forwards.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! `ForwardsApi` has no committed fixture under `tests/fixtures/`, so every test here builds a
//! small inline `{"meta": {...}, "data": {...}}` body with `serde_json::json!`, shaped like the
//! payloads `eero-api`'s own tests use (`tests/api/test_forwards.py`) but with obviously
//! synthetic MACs (`aa:bb:cc:00:00:0N`) and IPs drawn from the `192.0.2.0/24` documentation
//! range (RFC 5737) rather than anything real.

mod common;

use std::sync::Arc;

use rusteero::endpoints::forwards::ForwardsApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn forwards_api(mock: &MockEero) -> ForwardsApi {
    ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_forwards =====================

#[tokio::test]
async fn get_forwards_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": [
            {
                "port": 8080,
                "protocol": "tcp",
                "device_id": "device-0001",
                "ip": "192.0.2.11",
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.get_forwards("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== create_forward =====================

#[tokio::test]
async fn create_forward_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "mac": "aa:bb:cc:00:00:02",
        "internal_ip": "192.0.2.20",
        "internal_port": 8080,
        "external_port": 80,
        "protocol": "tcp",
    });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "url": "/2.2/networks/network-0001/forwards/fwd-0001" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.create_forward("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== delete_forward =====================

#[tokio::test]
async fn delete_forward_hits_forward_id_path_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/forwards/fwd-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.delete_forward("network-0001", "fwd-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
