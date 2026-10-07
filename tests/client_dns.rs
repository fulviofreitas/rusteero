//! `Client` integration suite for the `DnsAPI` domain, v8.0.4: `network_id` resolution, cache
//! invalidation, and `parent=` forwarding from the cache (`Client::network_parent`).

mod common;

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
async fn dns_setter_invalidates_the_network_bucket() -> anyhow::Result<()> {
    // NOT a Rust-only divergence (correcting a stale comment here):
    // at v8.0.4, every one of Python's DNS/SQM/security setters calls
    // `self._invalidate_network_cache(network_id)` itself (`client.py:2094-2264` and friends) —
    // this crate once documented an "improvement" here describing a gap against an *older*
    // Python baseline that v8.0.4 already closed on its own side. This test still pins the
    // behaviour; it is simply parity with `client.py`, not a Rust-only fix.
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
    client.set_dns_caching(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_caching_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
    // `dns_network_with_settings_link.json`'s `url` field is on `/2.4` and it publishes a
    // `resources.settings` link also on `/2.4` — if `Client::set_dns_caching` builds and passes
    // `network_parent(&network_id)` from the fresh `get_network` cache entry, the write must
    // land on `/2.4/networks/network-0001/settings`, not the `/2.2` bare-id template.
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
        .and(body_json(serde_json::json!({ "dns": { "caching": true } })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_dns_caching(true, Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_caching_with_nothing_cached_falls_back_to_the_bare_id_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_dns_caching(true, Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_dns_settings_never_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    // Request 1: populates the `network[nid]` cache entry.
    client.get_network(Some("network-0001"), false).await?;
    // Request 2: `get_dns_settings` never reads or writes the `network` cache bucket at all —
    // it is always a live call, landing on the same path.
    client.get_dns_settings(Some("network-0001")).await?;
    // No request: served from the still-fresh cache populated by request 1 — proving
    // `get_dns_settings` did not invalidate it in between.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

/// Mounts the `GET network` (`expect(2)`) and `PUT settings` (`expect(1)`) mocks every DNS-setter
/// invalidation test below shares, and issues the first `GET` that populates `network[{nid}]`.
async fn mount_dns_setter_invalidation_mocks(mock: &MockEero) -> anyhow::Result<Client> {
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

    let client = client(mock).await;
    client.get_network(Some("network-0001"), false).await?;
    Ok(client)
}

/// Every DNS setter invalidates `network[{nid}]`, matching `client.py`'s own
/// `_invalidate_network_cache` call on each (`client.py:2094-2264`).
#[tokio::test]
async fn set_custom_dns_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = mount_dns_setter_invalidation_mocks(&mock).await?;
    client
        .set_custom_dns(&["1.1.1.1"], Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv4_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = mount_dns_setter_invalidation_mocks(&mock).await?;
    client
        .set_custom_dns_ipv4(&["1.1.1.1"], Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = mount_dns_setter_invalidation_mocks(&mock).await?;
    client
        .set_custom_dns_ipv6(&["2606:4700:4700::1111"], Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn clear_custom_dns_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = mount_dns_setter_invalidation_mocks(&mock).await?;
    client.clear_custom_dns(None, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = mount_dns_setter_invalidation_mocks(&mock).await?;
    client
        .set_dns_mode("auto", None, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
