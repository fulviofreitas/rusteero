//! HTTP integration tests for five single-`GET` endpoint modules that share one wire shape
//! (`GET /2.2/networks/{network_id}/<resource>`): `PasswordApi`, `ACCompatApi`, `RoutingApi`,
//! `ThreadApi` and `UpdatesApi` (`src/endpoints/password.rs`, `ac_compat.rs`, `routing.rs`,
//! `thread.rs`, `updates.rs`).
//!
//! None of these five Python modules (`eero-api src/eero/api/{password,ac_compat,routing,
//! thread,updates}.py`) has a fixture under `tests/fixtures/`, so each test builds a small,
//! synthetic `{"meta": …, "data": …}` body inline with `serde_json::json!`, matching the shape
//! exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_<module>.py::Test<Module>APIGet<Method>::test_get_<method>_returns_raw_response`).
//! No real MACs, serials, IPs or names appear in any fixture here.
//!
//! Per the crate's testing conventions, every test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body served via `into_value()` — the raw wire payload is the contract, never a reshaped
//! view of it. A dedicated test additionally pins the security guarantee that matters most for
//! `PasswordApi::get_password`: its response body (the network's Wi-Fi password) never appears
//! in the `Envelope`'s `Debug` output.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ac_compat::ACCompatApi;
use rusteero::endpoints::password::PasswordApi;
use rusteero::endpoints::routing::RoutingApi;
use rusteero::endpoints::thread::ThreadApi;
use rusteero::endpoints::updates::UpdatesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

// ===================== get_password =====================

#[tokio::test]
async fn get_password_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"password": "correct-horse-battery-staple", "ssid": "SynthNet"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = PasswordApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_password("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

/// Pins the guarantee `PasswordApi::get_password`'s doc comment promises: even though
/// `Envelope` carries the network's Wi-Fi password verbatim in `data`, its `Debug` impl
/// (`src/envelope.rs`) never renders the payload — only a `meta.code`/`data` "kind" summary —
/// so an incidental `{:?}` on the returned envelope can never leak the password into a log line.
#[tokio::test]
async fn get_password_debug_output_never_contains_the_password() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let secret_password = "correct-horse-battery-staple";
    let body = json!({
        "meta": {"code": 200},
        "data": {"password": secret_password, "ssid": "SynthNet"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = PasswordApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_password("network-0001").await?;

    let rendered = format!("{env:?}");
    assert!(!rendered.contains(secret_password));
    Ok(())
}

// ===================== get_ac_compat =====================

#[tokio::test]
async fn get_ac_compat_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"compatible": true, "devices": []},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ac_compat"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ACCompatApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_ac_compat("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

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

// ===================== get_thread =====================

#[tokio::test]
async fn get_thread_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"enabled": true, "devices": []},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ThreadApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_thread("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_updates =====================

#[tokio::test]
async fn get_updates_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"available": true, "current_version": "6.15.0"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = UpdatesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_updates("network-0001").await?;

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
