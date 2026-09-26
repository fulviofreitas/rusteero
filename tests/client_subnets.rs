//! `Client` integration suite for `SubnetsApi`'s network-id resolution and cache invalidation.

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

#[tokio::test]
async fn set_subnets_config_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_subnets_config(json!({"subnet_type": "guest"}), Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn delete_subnet_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/subnets_config/guest"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.delete_subnet("guest", Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_subnet_content_filters_does_not_invalidate_the_network_bucket() -> anyhow::Result<()> {
    // Deliberately different from `set_subnets_config` — matches Python exactly
    // (`client.py` never calls `_invalidate_network_cache` here).
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/subnets_config/dns_policies/content_filters",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_subnet_content_filters(json!({"content_filters": ["adult"]}), Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn get_subnets_config_prefers_the_cached_networks_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": { "code": 200 },
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "subnets_config": "/2.3/networks/network-0001/subnets_config",
            },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/subnets_config"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_subnets_config(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_subnet_content_filters_resolves_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/subnets_config/subnet_001/dns_policies/content_filters",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_subnet_content_filters("subnet_001", Some("network-0001"))
        .await?;
    Ok(())
}
