//! HTTP integration tests for `DataUsageApi` (`src/endpoints/data_usage.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! [`DataUsageApi::get_data_usage`] sends a `GET` with a JSON *body* — proven both by
//! `body_json` matchers and by a same-network mock on the *other* `data_usage`/`data_usage/
//! {resource}` path asserting `.expect(0)`, so a bug that renders the wrong path would fail
//! loudly instead of accidentally matching.

mod common;

use std::sync::Arc;

use rusteero::endpoints::data_usage::DataUsageApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn data_usage_api(mock: &MockEero) -> DataUsageApi {
    DataUsageApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_data_usage =====================

#[tokio::test]
async fn get_data_usage_without_resource_sends_the_body_to_the_bare_data_usage_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "timezone": "America/New_York", "period": "day" });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "download": 1_024_000, "upload": 512_000 },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // If `resource: None` ever accidentally rendered the resource-scoped route, this mock —
    // registered on the sibling path — would be the one to receive the request instead.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api.get_data_usage("network-0001", payload, None).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_data_usage_with_resource_sends_the_body_to_the_resource_scoped_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "timezone": "America/New_York" });
    let response = json!({
        "meta": { "code": 200 },
        "data": [{ "device_id": "dev_1", "download": 100 }],
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // If `resource: Some("devices")` ever accidentally rendered the bare route, this mock —
    // registered on the sibling path — would be the one to receive the request instead.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api
        .get_data_usage("network-0001", payload, Some("devices"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}
