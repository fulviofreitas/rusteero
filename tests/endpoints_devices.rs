//! HTTP integration tests for the read-only half of `DevicesApi` (`src/endpoints/devices.rs`):
//! `get_devices`, `get_device`, the `404` status mapping, and the path-segment safety of a
//! `device_id` that contains a `/`.
//!
//! Per the crate's testing conventions, every test here pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! its fixture via `into_value()` — the raw wire payload is the contract, never a reshaped view
//! of it.

mod common;

use std::sync::Arc;

use rusteero::endpoints::devices::DevicesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

// ===================== get_devices =====================

#[tokio::test]
async fn get_devices_hits_v22_path_with_session_cookie_and_matches_fixture() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_devices("network-0001").await?;

    assert_eq!(env.into_value(), fixture_json("devices.json"));
    Ok(())
}

// ===================== get_device =====================

#[tokio::test]
async fn get_device_hits_v22_path_with_session_cookie_and_matches_fixture() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_device("network-0001", "device-0001").await?;

    assert_eq!(env.into_value(), fixture_json("device.json"));
    Ok(())
}

// ===================== 404 status mapping =====================

#[tokio::test]
async fn get_device_unknown_id_maps_to_api_error_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-9999"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_string(r#"{"meta":{"code":404,"error":"not_found"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .get_device("network-0001", "device-9999")
        .await
        .expect_err("an unknown device id must map to Error::Api { status: 404, .. }");

    assert!(matches!(err, Error::Api { status: 404, .. }));
    Ok(())
}

// ===================== path-segment safety =====================

#[tokio::test]
async fn get_device_with_slash_in_device_id_cannot_traverse_out_of_its_path_segment()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // `routes::GET_DEVICE`'s renderer percent-encodes every segment for the path-segment
    // position, so a `/` inside `device_id` becomes a literal `%2F` rather than introducing a
    // new path segment. A `device_id` that looks like a traversal attempt therefore lands on
    // exactly one, single-segment path — never on `/2.2/networks/{network_id}/secrets` or any
    // other sibling resource.
    let malicious_id = "../../secrets";
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/..%2F..%2Fsecrets"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_device("network-0001", malicious_id).await?;

    assert_eq!(env.into_value(), fixture_json("device.json"));
    Ok(())
}

// ===================== set_device_nickname =====================
//
// Per the crate's testing conventions, every test below pins the exact verb, path and JSON body
// against a local `wiremock` server. Two of them additionally guard against `eero-api` issue
// #102: `set_device_nickname`/`pause_device` MUST land on `/2.3/...`, never the `/2.2/...`
// sibling that silently drops the write — each mounts an `.expect(0)` mock on the `/2.2/`
// equivalent path so a regression fails loudly.

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
//
// `block_device(true)` MUST be two round-trips (`GET` the device, then `POST` its resolved MAC
// to `/blacklist`), never a `PUT` on the device resource (`eero-api` issue #109).

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
