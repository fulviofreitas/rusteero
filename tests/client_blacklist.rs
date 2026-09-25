//! `Client` integration suite for `BlacklistApi`'s cache invalidation (security review finding
//! F2).

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
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

/// **F2**: `add_to_blacklist` issues the identical `POST .../blacklist` call
/// `block_device(blocked: true)` makes, and `block_device` already invalidates the `devices`
/// bucket on success. Before the fix, `add_to_blacklist` invalidated nothing, so a `get_devices`
/// call right after would keep reporting the device as unblocked for the rest of the TTL.
#[tokio::test]
async fn add_to_blacklist_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_devices(Some("network-0001"), false).await?;
    client
        .add_to_blacklist("AA:BB:CC:00:00:01", Some("network-0001"))
        .await?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}

/// **F2**: `remove_from_blacklist` must invalidate the `devices` bucket too, for the same reason
/// as `add_to_blacklist` above.
#[tokio::test]
async fn remove_from_blacklist_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/blacklist/AA:BB:CC:00:00:01",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_devices(Some("network-0001"), false).await?;
    client
        .remove_from_blacklist("AA:BB:CC:00:00:01", Some("network-0001"))
        .await?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}
