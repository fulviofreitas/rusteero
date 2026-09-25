//! HTTP integration tests for `ReservationsApi` (`src/endpoints/reservations.rs`) against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! `ReservationsApi` has no committed fixture under `tests/fixtures/`, so every test here builds
//! a small inline `{"meta": {...}, "data": {...}}` body with `serde_json::json!`, shaped like the
//! payloads `eero-api`'s own tests use (`tests/api/test_reservations.py`) but with obviously
//! synthetic MACs (`aa:bb:cc:00:00:0N`) and IPs drawn from the `192.0.2.0/24` documentation
//! range (RFC 5737) rather than anything real.
//!
//! `create_reservation_passthrough_body_arrives_byte_identical_including_nested_object` carries
//! a nested object to prove a passthrough body arrives byte-identical, nesting included; it was
//! red-green verified by hand (see this crate's task report for the observed RED output).

mod common;

use std::sync::Arc;

use rusteero::endpoints::reservations::ReservationsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn reservations_api(mock: &MockEero) -> ReservationsApi {
    ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_reservations =====================

#[tokio::test]
async fn get_reservations_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": [
            { "ip": "192.0.2.10", "mac": "aa:bb:cc:00:00:01" },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.get_reservations("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_reservations_with_unknown_network_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/reservations"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(404).set_body_string(
                json!({ "meta": { "code": 404, "error": "not_found" } }).to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let err = api
        .get_reservations("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== create_reservation =====================

#[tokio::test]
async fn create_reservation_passthrough_body_arrives_byte_identical_including_nested_object()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "mac": "aa:bb:cc:00:00:01",
        "ip": "192.0.2.10",
        "name": "test-reservation",
        "schedule": {
            "enabled": true,
            "days": ["mon", "tue"],
            "window": { "start": "22:00", "end": "06:00" },
        },
    });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "url": "/2.2/networks/network-0001/reservations/res-0001" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.create_reservation("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== update_reservation =====================

#[tokio::test]
async fn update_reservation_puts_passthrough_body_to_reservation_id_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "name": "renamed-reservation", "ip": "192.0.2.11" });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .update_reservation("network-0001", "res-0001", payload)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== delete_reservation =====================

#[tokio::test]
async fn delete_reservation_hits_reservation_id_path_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.delete_reservation("network-0001", "res-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
