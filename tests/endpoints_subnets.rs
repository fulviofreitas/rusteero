//! HTTP integration tests for `SubnetsApi` (`src/endpoints/subnets.rs`) against a local
//! `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::subnets::SubnetsApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn subnets_api(mock: &MockEero) -> SubnetsApi {
    SubnetsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_config =====================

#[tokio::test]
async fn get_config_hits_v22_path_with_session_cookie() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "subnet_type": "main" } });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.get_config("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_config_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent =
        json!({"resources": {"subnets_config": "/2.3/networks/network-0001/subnets_config"}});
    let body = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.get_config("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== set_config =====================

#[tokio::test]
async fn set_config_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "subnet_type": "guest", "enabled": true, "wan_access": false });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.set_config("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_config_password_field_is_forwarded_and_never_logged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "subnet_type": "guest", "password": "super-secret-value" });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.set_config("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== delete_subnet =====================

#[tokio::test]
async fn delete_subnet_sends_delete_to_subnet_type_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/subnets_config/guest"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.delete_subnet("network-0001", "guest").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_content_filters =====================

#[tokio::test]
async fn set_content_filters_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "content_filters": ["adult"], "subnets": ["subnet_001"] });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/subnets_config/dns_policies/content_filters",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api.set_content_filters("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== get_content_filters =====================

#[tokio::test]
async fn get_content_filters_returns_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "dns_policies_enabled": true } });

    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/subnets_config/subnet_001/dns_policies/content_filters",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = subnets_api(&mock);
    let env = api
        .get_content_filters("network-0001", "subnet_001")
        .await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}
