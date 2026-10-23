//! `Client` integration suite for `WanApi`'s network-id resolution and cache invalidation.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie, user_token_header};
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
async fn get_multistaticip_prefers_the_cached_networks_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": { "code": 200 },
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "multistaticip": "/2.3/networks/network-0001/multistaticip",
            },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_multistaticip(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn set_multistaticip_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_multistaticip(json!({"enabled": true}), Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_secondary_wan_config_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.3/networks/network-0001/devices/secondary_wan_config",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_secondary_wan_config(json!({"devices": []}), Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_device_secondary_wan_access_invalidates_the_device_bucket_not_the_network_bucket()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_device_secondary_wan_access("aa:bb:cc:00:00:01", true, Some("network-0001"))
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    // The network bucket is untouched: a second `get_network` would hit the mock again if it
    // had been invalidated, and the `.expect(1)` above would then fail on drop.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== version pins =====================

/// A cached network that publishes its `multistaticip` link on 2.2 still leads to a read on 2.3.
#[tokio::test]
async fn get_multistaticip_pins_to_2_3_when_the_cached_network_publishes_a_2_2_link()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": { "code": 200 },
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "multistaticip": "/2.2/networks/network-0001/multistaticip",
            },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/multistaticip"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_multistaticip(Some("network-0001")).await?;
    Ok(())
}

/// An explicit network given as a 2.2 path is re-pinned to 2.3 by all three writes.
#[tokio::test]
async fn the_wan_writes_pin_v2_3_for_a_path_form_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for tail in [
        "multistaticip",
        "devices/secondary_wan_config",
        "devices/aa:bb:cc:00:00:01",
    ] {
        Mock::given(method("PUT"))
            .and(path(format!("/2.2/networks/network-0001/{tail}")))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/2.3/networks/network-0001/{tail}")))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(
                ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()),
            )
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let client = client(&mock).await;
    let network = Some("/2.2/networks/network-0001");
    client
        .set_multistaticip(json!({"enabled": true}), network)
        .await?;
    client
        .set_secondary_wan_config(json!({"devices": []}), network)
        .await?;
    client
        .set_device_secondary_wan_access("aa:bb:cc:00:00:01", true, network)
        .await?;
    Ok(())
}
