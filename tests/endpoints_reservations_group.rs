//! HTTP integration tests for the read-only (`GET`) half of four small `eero-api` domain
//! modules that share no fixtures of their own: `ReservationsApi::get_reservations`,
//! `ForwardsApi::get_forwards`, `SupportApi::get_support`, and
//! `TransferApi::get_transfer_stats` (both branches — network-level and device-level).
//!
//! None of these four modules has a committed fixture under `tests/fixtures/`, so every test
//! here builds a small inline `{"meta": {...}, "data": {...}}` body with `serde_json::json!`,
//! shaped like the payloads `eero-api`'s own tests use
//! (`tests/api/test_reservations.py`, `test_forwards.py`, `test_transfer.py`,
//! `test_support.py`) but with obviously synthetic MACs (`aa:bb:cc:00:00:0N`) and IPs drawn
//! from the `192.0.2.0/24` documentation range (RFC 5737) rather than anything real.
//!
//! Per the crate's testing conventions, every test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body it was served via `into_value()` — the raw wire payload is the contract, never a
//! reshaped view of it. The two `get_transfer_stats` tests each additionally mount a
//! `.expect(0)` mock on the *other* branch's path, so a regression that renders the wrong route
//! for a given `device_id` fails loudly instead of silently matching the wrong mock.

mod common;

use std::sync::Arc;

use rusteero::endpoints::forwards::ForwardsApi;
use rusteero::endpoints::reservations::ReservationsApi;
use rusteero::endpoints::support::SupportApi;
use rusteero::endpoints::transfer::TransferApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

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

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_reservations("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_forwards =====================

#[tokio::test]
async fn get_forwards_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": [
            {
                "port": 8080,
                "protocol": "tcp",
                "device_id": "device-0001",
                "ip": "192.0.2.11",
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_forwards("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_support =====================

#[tokio::test]
async fn get_support_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": { "phone": "+15555550100", "email": "support@example.com" },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = SupportApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_support("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_transfer_stats =====================

#[tokio::test]
async fn get_transfer_stats_with_none_hits_the_network_level_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": { "total_download": 1_073_741_824_u64, "total_upload": 536_870_912_u64 },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/transfer"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // If `get_transfer_stats(None)` ever rendered the device-level path instead, this mock
    // would never be hit for the network-level call above and this `.expect(0)` would still be
    // satisfied — so it is the network-level mock's own `.expect(1)` that catches that
    // regression; this mock exists to catch the opposite mistake (both branches somehow hitting
    // the device path).
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0002/transfer",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = TransferApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_transfer_stats("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_transfer_stats_with_device_id_hits_the_device_level_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": {
            "device_id": "device-0002",
            "download": 104_857_600_u64,
            "upload": 52_428_800_u64,
        },
    });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0002/transfer",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // Guards against the opposite regression from the test above: a `device_id` that
    // accidentally gets routed to the network-level path instead of the device-level one.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/transfer"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = TransferApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api
        .get_transfer_stats("network-0001", Some("device-0002"))
        .await?;

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

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .get_reservations("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
