//! `Client` integration suite for the `SecurityAPI` domain, v8.0.4: `network_id` resolution,
//! cache invalidation, and `parent=` forwarding from the cache (`Client::network_parent`).

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
async fn set_wpa3_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_wpa3(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_wpa3_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
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
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_wpa3(true, Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_security_settings_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
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
    // `get_security_settings` prefers the parent's own `url` (self_url), on `/2.4` in this
    // fixture — not the parent-supplied `resources.settings` link, which is irrelevant to a read.
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_security_settings(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn configure_security_no_longer_accepts_a_thread_argument_and_still_invalidates()
-> anyhow::Result<()> {
    // Regression proof for the v8.0.0 signature change: `configure_security` has no `thread`
    // parameter any more (see `src/client/security.rs`'s own docs) — this test compiling at all
    // is part of the proof; the invalidation behaviour is asserted the same way as every other
    // setter in this suite.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .configure_security(Some(true), None, None, None, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_band_steering_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "band_steering": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_band_steering(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_upnp_forwards_the_cached_network_as_parent_and_invalidates_it() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "upnp": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_upnp(false, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_ipv6_forwards_the_cached_network_as_parent_and_invalidates_it() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_ipv6(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_mlo_mode_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/mlo_mode"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_mlo_mode("disabled", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_passpoint_enabled_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/passpoint/enabled"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_passpoint_enabled(true, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_proxied_nodes_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/proxied_nodes"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_proxied_nodes(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn get_fast_transition_never_invalidates_and_set_fast_transition_does() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/fast_transition"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({"meta": {"code": 200}, "data": {}}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/fast_transition"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({"meta": {"code": 200}, "data": {}}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_fast_transition(Some("network-0001")).await?;
    client
        .set_fast_transition(true, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
