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
    // Divergence from eero-api (rust-port-plan.md §3.8, improvement (a)): Python's DNS/SQM/
    // security setters invalidate nothing, even though they PUT the exact resource `get_network`
    // caches.
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
