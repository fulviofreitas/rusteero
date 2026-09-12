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
use wiremock::matchers::{method, path};
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
