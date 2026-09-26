//! `Client` integration suite for `DataUsageApi`'s pass-throughs at `v8.0.4`.
//!
//! None of these eleven methods is cached; `set_data_usage_report_settings` is the only write
//! and is the only one that invalidates anything.

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
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
async fn get_data_usage_forwards_every_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("timezone", "UTC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage("s", "e", "daily", Some("UTC"), Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_data_usage_breakdown_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/breakdown"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage_breakdown("s", "e", None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_data_usage_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices_data_usage("s", "e", None, None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_data_usage_hits_the_per_device_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/devices/aa:bb:cc",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_device_data_usage("aa:bb:cc", "s", "e", "daily", None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eeros_data_usage_summary_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros/summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_eeros_data_usage_summary("s", "e", "daily", None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_data_usage_hits_the_per_eero_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_eero_data_usage("eero-0001", "s", "e", "daily", None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_data_usage_hits_the_per_profile_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/profiles/profile-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile_data_usage(
            "profile-0001",
            "s",
            "e",
            "daily",
            None,
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_devices_data_usage_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/devices",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_unprofiled_devices_data_usage("s", "e", None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_data_usage_summary_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/summary",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_unprofiled_data_usage_summary("s", "e", "daily", None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_data_usage_report_settings_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage_report_settings(Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_data_usage_report_settings_invalidates_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({"meta": {"code": 200}, "data": {"id": "network-0001"}}).to_string(),
        ))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(body_json(
            json!({ "cadence": "daily", "notification_day": "monday" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_data_usage_report_settings("daily", "monday", Some("network-0001"))
        .await?;
    // `network[{nid}]` was invalidated by the write above, so this second read must hit the
    // network again.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
