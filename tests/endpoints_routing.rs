//! HTTP integration tests for `RoutingApi` (`src/endpoints/routing.rs`).
//!
//! `RoutingApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_routing.py`). No real MACs, serials, IPs or names appear in any fixture
//! here.

mod common;

use std::sync::Arc;

use rusteero::endpoints::routing::RoutingApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn routing_api(mock: &MockEero) -> RoutingApi {
    RoutingApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_routing =====================

#[tokio::test]
async fn get_routing_bare_id_builds_the_default_v22_template_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"routes": [], "mode": "automatic"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = routing_api(&mock).get_routing("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_routing_prefers_the_parents_routing_link_even_on_a_different_api_version()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"routes": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    // The parent's own `routing` link encodes API version 2.3 directly in its path — proving the
    // link, when present, is used verbatim rather than forced back onto the 2.2 template default.
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"routing": "/2.3/networks/network-0001/routing"}});
    let env = routing_api(&mock)
        .get_routing("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_routing_with_unknown_network_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = routing_api(&mock);
    let err = api
        .get_routing("does-not-exist", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
