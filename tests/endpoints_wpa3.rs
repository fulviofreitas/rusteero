//! HTTP integration tests for `Wpa3Api` (`src/endpoints/wpa3.rs`), new in v8.0.0, against a
//! local `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::wpa3::Wpa3Api;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds a [`Wpa3Api`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn wpa3_api(mock: &MockEero) -> Wpa3Api {
    Wpa3Api::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

// ===================== get_wpa3_per_band =====================

#[tokio::test]
async fn get_wpa3_per_band_hits_the_sub_resource_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/wpa3_per_band"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("wpa3_per_band.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wpa3_api(&mock);
    let env = api.get_wpa3_per_band("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("wpa3_per_band.json"));
    Ok(())
}

#[tokio::test]
async fn get_wpa3_per_band_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/wpa3_per_band"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("wpa3_per_band.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = wpa3_api(&mock);
    api.get_wpa3_per_band("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== set_wpa3_per_band =====================

#[tokio::test]
async fn set_wpa3_per_band_sends_only_the_given_band() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for mode in ["WPA2", "WPA2_WPA3", "WPA3"] {
        Mock::given(method("PUT"))
            .and(path("/2.2/networks/network-0001/wpa3_per_band"))
            .and(body_json(json!({ "band_2_4_ghz": mode })))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let api = wpa3_api(&mock);
    for mode in ["WPA2", "WPA2_WPA3", "WPA3"] {
        api.set_wpa3_per_band("network-0001", Some(mode), None, None)
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn set_wpa3_per_band_sends_both_bands() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/wpa3_per_band"))
        .and(body_json(json!({
            "band_2_4_ghz": "WPA2_WPA3",
            "band_5_ghz": "WPA3",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wpa3_api(&mock);
    api.set_wpa3_per_band("network-0001", Some("WPA2_WPA3"), Some("WPA3"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_wpa3_per_band_rejects_an_invalid_mode_with_no_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = wpa3_api(&mock);
    let err = api
        .set_wpa3_per_band("network-0001", Some("wpa3"), None, None)
        .await
        .expect_err("a lower-cased mode must be rejected");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "band_2_4_ghz"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_wpa3_per_band_rejects_no_fields_supplied_with_no_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = wpa3_api(&mock);
    let err = api
        .set_wpa3_per_band("network-0001", None, None, None)
        .await
        .expect_err("at least one band must be supplied");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "wpa3_per_band"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_wpa3_per_band_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/wpa3_per_band"))
        .and(body_json(json!({ "band_2_4_ghz": "WPA3" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = wpa3_api(&mock);
    api.set_wpa3_per_band("network-0001", Some("WPA3"), None, Some(&parent))
        .await?;
    Ok(())
}
