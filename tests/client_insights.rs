//! `Client` integration suite for `InsightsApi`'s pass-throughs at `v8.0.4`.
//!
//! `Client::run_insights` was removed at v8.0.4 (no replacement API operation) — see
//! `src/client/insights.rs`'s module docs.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_insights_forwards_every_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights"))
        .and(session_cookie())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("insight_type", "adblock"))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_insights(Some("network-0001"), "s", "e", "adblock", "daily")
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_insights_passes_the_cached_network_envelope_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_body = json!({
        "meta": {"code": 200},
        "data": {"id": "network-0001", "resources": {"insights_devices": "/2.4/networks/network-0001/insights/devices"}},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(network_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/insights/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    // Populate the `network[{nid}]` cache entry the parent-resolution helper reads.
    client.get_network(Some("network-0001"), false).await?;
    client
        .get_devices_insights(Some("network-0001"), "s", "e", "daily", "adblock")
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_insights_hits_the_per_device_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/insights/devices/device-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_device_insights(
            "device-0001",
            Some("network-0001"),
            "s",
            "e",
            "daily",
            "adblock",
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profiles_insights_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights/profiles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profiles_insights(Some("network-0001"), "s", "e", "daily", "adblock")
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_insights_hits_the_per_profile_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/insights/profiles/profile-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile_insights(
            "profile-0001",
            Some("network-0001"),
            "s",
            "e",
            "daily",
            "adblock",
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_devices_insights_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/insights/profiles/profile-0001/devices",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile_devices_insights(
            "profile-0001",
            Some("network-0001"),
            "s",
            "e",
            "daily",
            "adblock",
        )
        .await?;
    Ok(())
}
