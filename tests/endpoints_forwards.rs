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
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.get_forwards("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_forwards_prefers_the_parents_published_link_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"forwards": "/2.3/networks/network-0001/forwards"}});
    let body = json!({ "meta": { "code": 200 }, "data": [] });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/forwards"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.get_forwards("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_forwards_falls_back_to_template_when_parent_has_no_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {}});
    let body = json!({ "meta": { "code": 200 }, "data": [] });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.get_forwards("network-0001", Some(&parent)).await?;

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
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.create_forward("network-0001", payload, None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_forward_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({"mac": "aa:bb:cc:00:00:02"});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/forwards"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"forwards": "/2.3/networks/network-0001/forwards"}});
    let api = forwards_api(&mock);
    let env = api
        .create_forward("network-0001", payload, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== update_forward =====================

#[tokio::test]
async fn update_forward_from_path_string_needs_no_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "enabled": false });
    let response = json!({ "meta": { "code": 200 }, "data": { "enabled": false } });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/forwards/fwd-0001"))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_json(payload.clone()))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api
        .update_forward(
            "/2.2/networks/network-0001/forwards/fwd-0001",
            payload,
            None,
            None,
        )
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_forward_from_envelope_uses_its_own_url_even_on_a_different_version()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"url": "/2.3/networks/network-0001/forwards/fwd-0001"});
    let payload = json!({ "enabled": true });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/forwards/fwd-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api
        .update_forward("fwd-0001", payload, None, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_forward_bare_id_requires_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = forwards_api(&mock);

    let err = api
        .update_forward("fwd-0001", json!({}), None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "network"));
    Ok(())
}

#[tokio::test]
async fn update_forward_bare_id_with_network_resolves_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/forwards/fwd-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api
        .update_forward("fwd-0001", json!({}), Some("network-0001"), None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_forward_envelope_with_no_url_is_a_validation_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"id": "fwd-0001"});
    let api = forwards_api(&mock);

    let err = api
        .update_forward("fwd-0001", json!({}), None, Some(&parent))
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "forward"));
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.delete_forward("network-0001", "fwd-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
