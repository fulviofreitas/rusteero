//! `Client` integration suite for `ProfilesApi`'s pass-throughs and cache invalidation at
//! v8.0.4.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use serde_json::json;

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

// ===================== One representative mutation =====================

#[tokio::test]
async fn pause_profile_reaches_the_profile_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .pause_profile("profile-0001", true, Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("profile.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================

#[tokio::test]
async fn rename_profile_invalidates_the_profiles_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .rename_profile("profile-0001", "New Name", Some("network-0001"))
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn create_profile_invalidates_only_the_profiles_list_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .create_profile("Guests", None, None, Some("network-0001"))
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn create_profile_includes_devices_and_paused_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Guests",
            "devices": [{ "url": "/2.2/networks/network-0001/devices/device-0001" }],
            "paused": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .create_profile(
            "Guests",
            Some(&["/2.2/networks/network-0001/devices/device-0001"]),
            Some(true),
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

// ===================== set_profile_devices: invalidates BOTH profile keys =====================

/// `set_profile_devices` invalidates both `profiles[{nid}_{pid}]` AND `profiles[{nid}_profiles]`
/// — Python's `_invalidate_profile_cache` (`client.py:1012-1020`, called at `:2289`) drops both
/// keys, not just the single-profile one (see `Client::set_profile_devices`'s own docs).
#[tokio::test]
async fn set_profile_devices_invalidates_both_the_single_profile_and_list_keys()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .set_profile_devices(
            "profile-0001",
            &["/2.2/networks/network-0001/devices/device-0001"],
            Some("network-0001"),
        )
        .await?;
    // Both caches were dropped by the write: both reads must hit the network again, not the
    // (now-stale) cache.
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}
