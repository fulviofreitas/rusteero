//! HTTP integration tests for `OUICheckApi` (`src/endpoints/ouicheck.rs`), pinned against
//! `eero-api src/eero/api/ouicheck.py` at v8.0.4.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ouicheck::OUICheckApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn ouicheck_api(mock: &MockEero) -> OUICheckApi {
    OUICheckApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_ouicheck =====================

#[tokio::test]
async fn get_ouicheck_sends_serial_and_version_as_query_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "vendor": "ExampleVendor" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(query_param("serial", "ABC123"))
        .and(query_param("version", "1"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api
        .get_ouicheck("network-0001", "ABC123", "1", None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_prefers_the_parents_own_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/ouicheck"))
        .and(query_param("serial", "ABC123"))
        .and(query_param("version", "1"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {}, "data": {}})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let api = ouicheck_api(&mock);
    api.get_ouicheck("network-0001", "ABC123", "1", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_rejects_an_empty_serial_before_any_request_is_sent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = ouicheck_api(&mock);
    let err = api
        .get_ouicheck("network-0001", "", "1", None)
        .await
        .expect_err("an empty serial must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "serial"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_rejects_an_empty_version_before_any_request_is_sent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = ouicheck_api(&mock);
    let err = api
        .get_ouicheck("network-0001", "ABC123", "", None)
        .await
        .expect_err("an empty version must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "version"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_empty_body_response_is_still_a_valid_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(session_cookie())
        .and(query_param_is_missing("resource"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api
        .get_ouicheck("network-0001", "ABC123", "1", None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}
