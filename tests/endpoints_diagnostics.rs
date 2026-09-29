//! HTTP integration tests for `DiagnosticsApi` (`src/endpoints/diagnostics.rs`), pinned against
//! `eero-api src/eero/api/diagnostics.py` at v8.0.4.

mod common;

use std::sync::Arc;

use rusteero::endpoints::diagnostics::DiagnosticsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn diagnostics_api(mock: &MockEero) -> DiagnosticsApi {
    DiagnosticsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_diagnostics =====================

#[tokio::test]
async fn get_diagnostics_uses_the_default_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "network_health": "good", "internet_status": "connected" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api.get_diagnostics("network-0001", None).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_diagnostics_prefers_a_parent_supplied_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {}, "data": {}})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"diagnostics": "/2.3/networks/network-0001/diagnostics"}});
    let api = diagnostics_api(&mock);
    api.get_diagnostics("network-0001", Some(&parent)).await?;
    Ok(())
}

#[tokio::test]
async fn get_diagnostics_with_unknown_network_maps_404_to_not_found() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let err = api
        .get_diagnostics("does-not-exist", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== run_diagnostics =====================

#[tokio::test]
async fn run_diagnostics_sends_an_empty_object_with_no_args() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api
        .run_diagnostics("network-0001", None, None, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn run_diagnostics_sends_only_the_supplied_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "device": "dev-0001" })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    api.run_diagnostics("network-0001", Some("dev-0001"), None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn run_diagnostics_sends_both_supplied_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({ "device": "dev-0001", "symptom": "slow" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    api.run_diagnostics("network-0001", Some("dev-0001"), Some("slow"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn run_diagnostics_prefers_a_parent_supplied_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"diagnostics": "/2.3/networks/network-0001/diagnostics"}});
    let api = diagnostics_api(&mock);
    api.run_diagnostics("network-0001", None, None, Some(&parent))
        .await?;
    Ok(())
}
