//! HTTP integration tests for `DiagnosticsApi` (`src/endpoints/diagnostics.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! A final error-path test proves a `404` maps to `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::diagnostics::DiagnosticsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn diagnostics_api(mock: &MockEero) -> DiagnosticsApi {
    DiagnosticsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_diagnostics =====================

#[tokio::test]
async fn get_diagnostics_hits_networks_id_diagnostics_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "network_health": "good", "internet_status": "connected" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api.get_diagnostics("network-0001").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_diagnostics_with_unknown_network_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/diagnostics"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let err = api
        .get_diagnostics("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== run_diagnostics =====================

#[tokio::test]
async fn run_diagnostics_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api.run_diagnostics("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
