//! HTTP integration tests for `OUICheckApi` (`src/endpoints/ouicheck.rs`) against a local
//! `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ouicheck::OUICheckApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn ouicheck_api(mock: &MockEero) -> OUICheckApi {
    OUICheckApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_ouicheck =====================

#[tokio::test]
async fn get_ouicheck_hits_networks_id_ouicheck_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "vendor": "Apple", "mac": "AA:BB:CC:DD:EE:FF" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api.get_ouicheck("network-0001").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== run_ouicheck =====================

#[tokio::test]
async fn run_ouicheck_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api.run_ouicheck("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
