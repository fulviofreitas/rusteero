//! HTTP integration tests for `DevicesApi` (`src/endpoints/devices.rs`) at `v8.0.4`.
//!
//! Per the crate's testing conventions, every test here pins the exact verb, path, query/body
//! and session credentials against a local `wiremock` server, and asserts the returned
//! `Envelope` is byte-identical to its fixture via `into_value()` — the raw wire payload is the
//! contract, never a reshaped view of it.

mod common;

use std::sync::Arc;

use rusteero::endpoints::devices::DevicesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

fn devices_api(mock: &MockEero) -> DevicesApi {
    DevicesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_devices =====================

#[tokio::test]
async fn get_devices_hits_v22_path_with_credentials_and_matches_fixture() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    let env = api.get_devices("network-0001", None, None, None).await?;

    assert_eq!(env.into_value(), fixture_json("devices.json"));
    Ok(())
}

#[tokio::test]
async fn get_devices_sends_thread_and_proxied_node_as_query_params_when_supplied()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(query_param("thread", "true"))
        .and(query_param("proxied_node", "false"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.get_devices("network-0001", Some(true), Some(false), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_prefers_parents_devices_link_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/devices"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    // The bare-id template must never be hit when a parent link is present.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"devices": "/2.4/networks/network-0001/devices"}});
    let api = devices_api(&mock);
    api.get_devices("network-0001", None, None, Some(&parent))
        .await?;
    Ok(())
}

// ===================== get_device =====================

#[tokio::test]
async fn get_device_hits_v22_path_with_credentials_and_matches_fixture() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    let env = api.get_device("network-0001", "device-0001", None).await?;

    assert_eq!(env.into_value(), fixture_json("device.json"));
    Ok(())
}

#[tokio::test]
async fn get_device_unknown_id_maps_to_api_error_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-9999"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_string(r#"{"meta":{"code":404,"error":"not_found"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    let err = api
        .get_device("network-0001", "device-9999", None)
        .await
        .expect_err("an unknown device id must map to Error::NotFound { status: 404, .. }");

    assert!(matches!(err, Error::NotFound { status: 404, .. }));
    Ok(())
}

#[tokio::test]
async fn get_device_prefers_parents_self_url_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.4/networks/network-0001/devices/device-0001"});
    let api = devices_api(&mock);
    api.get_device("network-0001", "device-0001", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_falls_back_to_the_template_when_parent_has_no_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({});
    let api = devices_api(&mock);
    api.get_device("network-0001", "device-0001", Some(&parent))
        .await?;
    Ok(())
}

// ===================== path-segment safety =====================

#[tokio::test]
async fn get_device_with_dot_dot_mac_is_rejected_before_any_request() -> anyhow::Result<()> {
    // Ported from `DevicesAPI._device_url` -> `resolve_nested_url` -> `links.child_url` ->
    // `_validate_identifier` (`links.py:44-68`): a bare child id containing `".."` is rejected
    // with `EeroValidationException("id", ...)` *before* any request is built -- Python never
    // percent-encodes a traversal sequence into a single opaque path segment, it refuses to
    // resolve the URL at all. No mock is registered: any HTTP request at all is a failure here.
    let mock = MockEero::start().await;
    let malicious_id = "../../secrets";

    let api = devices_api(&mock);
    let err = api
        .get_device("network-0001", malicious_id, None)
        .await
        .expect_err("a \"..\"-bearing mac must be rejected, never resolved into a request");

    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

// ===================== set_device_nickname =====================

#[tokio::test]
async fn set_device_nickname_hits_v23_path_with_exact_body_never_v22() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    // Regression guard (eero-api issue #102): the identical PUT against /2.2 must never be sent.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "nickname": "Living Room TV" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.set_device_nickname("network-0001", "device-0001", "Living Room TV")
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_device_nickname_normalises_a_path_form_mac_and_still_targets_v2_3()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(body_json(json!({ "nickname": "Kitchen" })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.set_device_nickname(
        "network-0001",
        "/2.2/networks/network-0001/devices/device-0001",
        "Kitchen",
    )
    .await?;
    Ok(())
}

// ===================== pause_device =====================

#[tokio::test]
async fn pause_device_hits_v23_path_with_exact_body_never_v22() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.pause_device("network-0001", "device-0001", true)
        .await?;
    Ok(())
}

// ===================== update_device_via_link =====================

#[tokio::test]
async fn update_device_via_link_builds_the_template_url_and_sends_only_supplied_fields()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "nickname": "New Name" })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.update_device_via_link(
        "network-0001",
        "device-0001",
        Some("New Name"),
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_device_via_link_prefers_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/devices/device-0001"))
        .and(body_json(json!({ "paused": true })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.4/networks/network-0001/devices/device-0001"});
    let api = devices_api(&mock);
    api.update_device_via_link(
        "network-0001",
        "device-0001",
        None,
        Some(true),
        None,
        Some(&parent),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_device_via_link_sends_all_three_fields_when_all_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(body_json(
            json!({ "nickname": "New Name", "paused": false, "profile": "profile-0001" }),
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.update_device_via_link(
        "network-0001",
        "device-0001",
        Some("New Name"),
        Some(false),
        Some("profile-0001"),
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_device_via_link_with_no_fields_raises_validation_error_before_any_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    let err = api
        .update_device_via_link("network-0001", "device-0001", None, None, None, None)
        .await
        .expect_err("no fields supplied must raise Error::Validation before any request");

    assert!(matches!(err, Error::Validation { field, .. } if field == "device"));
    Ok(())
}

// ===================== set_device_type =====================

#[tokio::test]
async fn set_device_type_sends_the_expected_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .and(body_json(json!({ "device_type": "computer" })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.set_device_type("network-0001", "device-0001", "computer")
        .await?;
    Ok(())
}

// ===================== get_device_labels / set_device_labels =====================

#[tokio::test]
async fn get_device_labels_hits_the_labels_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({"meta": {"code": 200}, "data": {"make_label": "Acme"}});
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0001/labels",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    let env = api.get_device_labels("network-0001", "device-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn set_device_labels_sends_query_params_and_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0001/labels",
        ))
        .and(query_param("make_label", "Acme"))
        .and(query_param("type_label", "phone"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0001/labels",
        ))
        .and(query_param("make_label", "Acme"))
        .and(query_param("type_label", "phone"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.set_device_labels(
        "network-0001",
        "device-0001",
        Some("Acme"),
        None,
        None,
        Some("phone"),
    )
    .await?;
    Ok(())
}

// ===================== block_device / unblock_device (pure delegates) =====================

#[tokio::test]
async fn block_device_posts_form_encoded_mac_to_blacklist_with_no_device_get() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(wiremock::matchers::header(
            "content-type",
            "application/x-www-form-urlencoded",
        ))
        .and(wiremock::matchers::body_string("mac=device-0001"))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.block_device("network-0001", "device-0001").await?;
    Ok(())
}

#[tokio::test]
async fn unblock_device_deletes_from_blacklist_with_no_device_get() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/device-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = devices_api(&mock);
    api.unblock_device("network-0001", "device-0001").await?;
    Ok(())
}
