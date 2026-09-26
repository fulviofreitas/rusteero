//! HTTP integration tests for `WanApi` (`src/endpoints/wan.rs`) against a local `wiremock`
//! server per the crate's testing conventions. Every endpoint here is served on API version 2.3.

mod common;

use std::sync::Arc;

use rusteero::endpoints::wan::WanApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn wan_api(mock: &MockEero) -> WanApi {
    WanApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_multistaticip =====================

#[tokio::test]
async fn get_multistaticip_uses_version_2_3() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "enabled": true, "type": "P" } });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.get_multistaticip("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_multistaticip_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent =
        json!({"resources": {"multistaticip": "/2.3/networks/network-0001/multistaticip"}});
    let body = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.get_multistaticip("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_multistaticip_raises_not_found_on_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {
            "code": 404,
            "error": "error.network.multistaticip_not_found",
            "message": "error.network.multistaticip_not_found",
        },
        "data": null,
    });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let err = api
        .get_multistaticip("network-0001", None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NotFound { status: 404, .. }));
    Ok(())
}

// ===================== set_multistaticip =====================

#[tokio::test]
async fn set_multistaticip_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "enabled": true,
        "type": "P",
        "multistaticip_settings": { "router_ip": "203.0.113.1" },
    });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.set_multistaticip("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_secondary_wan_config =====================

#[tokio::test]
async fn set_secondary_wan_config_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "devices": [{ "mac": "aa:bb:cc:00:00:01", "secondary_wan_deny_access": true }],
    });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path(
            "/2.3/networks/network-0001/devices/secondary_wan_config",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_secondary_wan_config("network-0001", payload)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_device_secondary_wan_access =====================

#[tokio::test]
async fn set_device_secondary_wan_access_deny_true_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01"))
        .and(session_cookie())
        .and(body_json(json!({ "secondary_wan_deny_access": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_device_secondary_wan_access("network-0001", "aa:bb:cc:00:00:01", true)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_device_secondary_wan_access_deny_false_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01"))
        .and(session_cookie())
        .and(body_json(json!({ "secondary_wan_deny_access": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_device_secondary_wan_access("network-0001", "aa:bb:cc:00:00:01", false)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
