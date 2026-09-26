//! `Client` integration suite for `DevicesApi`'s pass-throughs and cache invalidation at
//! `v8.0.4`.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

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

// ===================== get_devices: cache + filtered bypass =====================

#[tokio::test]
async fn get_devices_caches_the_unfiltered_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_with_a_filter_never_reads_or_writes_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(query_param("thread", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    // Two filtered calls in a row must each hit the network — a filtered response is never
    // cached, so it can never serve the second call either.
    client
        .get_devices(Some("network-0001"), false, Some(true), None)
        .await?;
    client
        .get_devices(Some("network-0001"), false, Some(true), None)
        .await?;
    Ok(())
}

// ===================== One representative mutation =====================

#[tokio::test]
async fn set_device_nickname_reaches_the_v2_3_device_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_device_nickname("device-0001", "New Name", Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("device.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================

#[tokio::test]
async fn pause_device_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .pause_device("device-0001", true, Some("network-0001"))
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn block_device_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .block_device("device-0001", Some("network-0001"))
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn unblock_device_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .unblock_device("device-0001", Some("network-0001"))
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn update_device_via_link_invalidates_every_cached_profile_only_when_profile_supplied()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let profiles_body =
        serde_json::json!({"meta": {"code": 200}, "data": [{"id": "profile-0001"}]});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .respond_with(ResponseTemplate::new(200).set_body_string(profiles_body.to_string()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .update_device_via_link(
            "device-0001",
            None,
            None,
            Some("profile-0002"),
            Some("network-0001"),
        )
        .await?;
    // The `profiles` list bucket was invalidated by the write above, so this second read must
    // hit the network again.
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_device_type_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .set_device_type("device-0001", "computer", Some("network-0001"))
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_priority_never_touches_the_devices_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices/device-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    // Two consecutive calls must each reach the network — `get_device_priority` deliberately
    // never reads or writes `devices[{nid}_{did}]`.
    client
        .get_device_priority("device-0001", Some("network-0001"))
        .await?;
    client
        .get_device_priority("device-0001", Some("network-0001"))
        .await?;
    Ok(())
}

/// `get_device_priority` resolves `network_id` with `auto_discover = false` (`client.py:2199`) —
/// unlike most `devices` wrappers, it never falls back to probing `/networks` when no
/// `network_id`/preferred network is set.
#[tokio::test]
async fn get_device_priority_resolves_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_device_priority("device-0001", None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}
