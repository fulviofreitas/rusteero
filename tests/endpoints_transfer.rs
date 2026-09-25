//! HTTP integration tests for `TransferApi` (`src/endpoints/transfer.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! The two `get_transfer_stats` tests each additionally mount a `.expect(0)` mock on the
//! *other* branch's path, so a regression that renders the wrong route for a given `device_id`
//! fails loudly instead of silently matching the wrong mock.

mod common;

use std::sync::Arc;

use rusteero::endpoints::transfer::TransferApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

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
