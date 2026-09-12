//! HTTP integration tests for the mutating half of `DevicesApi` and `BlacklistApi`
//! (`src/endpoints/devices.rs`, `src/endpoints/blacklist.rs`): `set_device_nickname`,
//! `pause_device`, `block_device`, `add_to_blacklist`, `remove_from_blacklist`.
//!
//! Per the crate's testing conventions, every test here pins the exact verb, path, JSON body and
//! session cookie against a local `wiremock` server. Two tests in this file additionally guard
//! against the two regressions this phase exists to prevent:
//!
//! - `set_device_nickname`/`pause_device` MUST land on `/2.3/...`, never the `/2.2/...` sibling
//!   that silently drops the write (`eero-api` issue #102) — each test below mounts an
//!   `.expect(0)` mock on the `/2.2/` equivalent path so a regression fails loudly.
//! - `block_device(true)` MUST be two round-trips (`GET` the device, then `POST` its resolved
//!   MAC to `/blacklist`), never a `PUT` on the device resource (`eero-api` issue #109).

mod common;

use std::sync::Arc;

use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::endpoints::devices::DevicesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};

// ===================== set_device_nickname =====================

#[tokio::test]
async fn set_device_nickname_hits_v23_path_with_exact_body_never_v22() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // Regression guard (eero-api issue #102): the identical PUT against /2.2 must never be
    // sent. If a future change routes this call through `ApiVersion::V2_2` instead of
    // `ApiVersion::V2_3`, this mock's `.expect(0)` fails the test loudly instead of the write
    // silently no-op-ing against a real server.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "nickname": "Living Room TV" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.set_device_nickname("network-0001", "device-0001", "Living Room TV")
        .await?;

    Ok(())
}

// ===================== pause_device =====================

#[tokio::test]
async fn pause_device_hits_v23_path_with_exact_body_never_v22() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // Regression guard (eero-api issue #102), same rationale as the nickname test above.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "paused": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.pause_device("network-0001", "device-0001", true)
        .await?;

    Ok(())
}

// ===================== block_device(true) =====================

#[tokio::test]
async fn block_device_true_gets_device_then_posts_resolved_mac_to_blacklist() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    // `device.json`'s `data.mac` is "AA:BB:CC:00:00:01" — the POST body below must carry
    // exactly that value, proving it was read from the GET response rather than fabricated or
    // derived from `device_id`.
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(body_json(json!({ "mac": "AA:BB:CC:00:00:01" })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.block_device("network-0001", "device-0001", true)
        .await?;

    Ok(())
}

// ===================== block_device(false) =====================

#[tokio::test]
async fn block_device_false_deletes_blacklist_entry_without_getting_device_first()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // Unblocking must NOT fetch the device first — `devices.py:184-186` deletes the blacklist
    // entry directly using the caller-supplied `device_id`, with no preceding GET.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.block_device("network-0001", "device-0001", false)
        .await?;

    Ok(())
}

// ===================== block_device(true), missing MAC =====================

#[tokio::test]
async fn block_device_true_with_no_mac_on_device_yields_502_and_never_posts() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0002"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"meta":{"code":200},"data":{"hostname":"no-mac-device"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .block_device("network-0001", "device-0002", true)
        .await
        .expect_err(
            "a device response with no 'mac' field must yield Error::Api { status: 502, .. }",
        );

    match err {
        Error::Api {
            status, message, ..
        } => {
            assert_eq!(status, 502);
            assert_eq!(
                message,
                "Device device-0002 response missing 'mac' field; cannot blacklist"
            );
        }
        other => panic!("expected Error::Api {{ status: 502, .. }}, got {other:?}"),
    }

    Ok(())
}

// ===================== add_to_blacklist =====================

#[tokio::test]
async fn add_to_blacklist_body_is_exactly_mac() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(body_json(json!({ "mac": "aa:bb:cc:00:00:01" })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.add_to_blacklist("network-0001", "aa:bb:cc:00:00:01")
        .await?;

    Ok(())
}

// ===================== remove_from_blacklist =====================

#[tokio::test]
async fn remove_from_blacklist_hits_the_right_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/aabbcc000001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.remove_from_blacklist("network-0001", "aabbcc000001")
        .await?;

    Ok(())
}
