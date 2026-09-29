//! HTTP integration tests for `SupportApi` (`src/endpoints/support.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! `request_support` carries a nested object to prove a passthrough body arrives byte-identical,
//! nesting included.

mod common;

use std::sync::Arc;

use rusteero::endpoints::support::SupportApi;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn support_api(mock: &MockEero) -> SupportApi {
    SupportApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_support =====================

#[tokio::test]
async fn get_support_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": { "phone": "+15555550100", "email": "support@example.com" },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = support_api(&mock);
    let env = api.get_support("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_support_prefers_the_parents_support_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.2/networks/network-0001/custom-support"}});
    let api = support_api(&mock);
    let env = api.get_support("network-0001", Some(&parent)).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== request_support =====================

#[tokio::test]
async fn request_support_passthrough_body_arrives_byte_identical_including_nested_object()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "issue": "wifi-dropping",
        "contact": { "email": "user@example.com", "phone": "+15555550100" },
    });
    let response = json!({ "meta": { "code": 200 }, "data": { "ticket_id": "tick-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = support_api(&mock);
    let env = api.request_support("network-0001", payload, None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn request_support_prefers_the_parents_support_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({"issue": "wifi-dropping"});
    let response = json!({ "meta": { "code": 200 }, "data": { "ticket_id": "tick-0002" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/custom-support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.2/networks/network-0001/custom-support"}});
    let api = support_api(&mock);
    let env = api
        .request_support("network-0001", payload, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
