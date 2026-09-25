//! HTTP integration tests for `RoutingApi` (`src/endpoints/routing.rs`).
//!
//! `RoutingApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_routing.py`). No real MACs, serials, IPs or names appear in any fixture
//! here.
//!
//! Per the crate's testing conventions, every test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body served via `into_value()` — the raw wire payload is the contract, never a reshaped
//! view of it. A final error-path test proves a `404` maps to `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::routing::RoutingApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

// ===================== get_routing =====================

#[tokio::test]
async fn get_routing_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"routes": [], "mode": "automatic"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = RoutingApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_routing("network-0001").await?;

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
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = RoutingApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .get_routing("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
