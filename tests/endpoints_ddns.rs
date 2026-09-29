//! HTTP integration tests for `DdnsApi` (`src/endpoints/ddns.rs`) against a local `wiremock`
//! server per the crate's testing conventions. Both writes are parameterless `PUT`s: no `json`
//! or `x-www-form-urlencoded` body, and no `Content-Type` header at all.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ddns::DdnsApi;
use serde_json::json;
use wiremock::matchers::{body_string, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn ddns_api(mock: &MockEero) -> DdnsApi {
    DdnsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== enable =====================

#[tokio::test]
async fn enable_sends_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/ddns/enable"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ddns_api(&mock);
    let env = api.enable("network-0001", None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn enable_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"ddns_enable": "/2.3/networks/network-0001/ddns/enable"}});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/ddns/enable"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ddns_api(&mock);
    let env = api.enable("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== disable =====================

#[tokio::test]
async fn disable_sends_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/ddns/disable"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ddns_api(&mock);
    let env = api.disable("network-0001", None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn disable_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"ddns_disable": "/2.3/networks/network-0001/ddns/disable"}});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/ddns/disable"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ddns_api(&mock);
    let env = api.disable("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
