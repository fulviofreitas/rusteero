//! `Client` integration suite for the `Wpa3API` domain (new in v8.0.0), v8.0.4: `network_id`
//! resolution, cache invalidation, and `parent=` forwarding from the cache
//! (`Client::network_parent`).

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};
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
async fn get_wpa3_per_band_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/wpa3_per_band"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("wpa3_per_band.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_wpa3_per_band(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn set_wpa3_per_band_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/wpa3_per_band"))
        .and(session_cookie())
        .and(body_json(json!({ "band_2_4_ghz": "WPA3" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({"meta": {"code": 200}, "data": {}}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_wpa3_per_band(Some("WPA3"), None, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
