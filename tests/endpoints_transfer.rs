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

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn transfer_api(mock: &MockEero) -> TransferApi {
    TransferApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0002/transfer",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = transfer_api(&mock);
    let env = api.get_transfer_stats("network-0001", None, None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_transfer_stats_prefers_the_parents_transfer_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "total_download": 1 } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/transfer"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-transfer"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"transfer": "/2.2/networks/network-0001/custom-transfer"}});
    let api = transfer_api(&mock);
    let env = api
        .get_transfer_stats("network-0001", None, Some(&parent))
        .await?;
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

    let api = transfer_api(&mock);
    let env = api
        .get_transfer_stats("network-0001", Some("device-0002"), None)
        .await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

/// Ported from `transfer.py:71`'s `if device_id:` — a truthy check, not `is None`. An empty
/// `device_id` must be treated exactly like `None`: network-level path, `parent` honoured.
#[tokio::test]
async fn get_transfer_stats_with_empty_device_id_hits_the_network_level_path() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "download": 1, "upload": 2 } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/transfer"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices//transfer"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = transfer_api(&mock);
    let env = api
        .get_transfer_stats("network-0001", Some(""), None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_transfer_stats_device_id_with_brace_does_not_break_the_template() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let api = transfer_api(&mock);
    let err = api
        .get_transfer_stats("network-0001", Some("device{0}002"), None)
        .await
        .expect_err("a stray brace in device_id must be rejected, not treated as a placeholder");
    assert!(matches!(err, rusteero::error::Error::Validation { .. }));
    Ok(())
}
