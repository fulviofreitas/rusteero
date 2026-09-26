//! `Client` integration suite for the `ThreadAPI` domain, v8.0.4: `network_id` resolution, cache
//! invalidation, and the `get_thread`-forwards/writes-never-forward `parent=` split
//! (`src/client/thread.rs`'s own module docs).

mod common;

use serde_json::json;
use wiremock::matchers::{body_string, method, path};
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
async fn get_thread_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
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
        .and(path("/2.4/networks/network-0001/thread"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("thread_status.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_thread(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn set_thread_enabled_ignores_the_cached_network_and_targets_the_literal_v22_path()
-> anyhow::Result<()> {
    // Even with a fresh network cached, `set_thread_enabled` must still target the literal
    // `/2.2` path — Thread writes never consult `parent` at all (`src/client/thread.rs`'s own
    // docs); nothing here depends on what happens to be cached.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(session_cookie())
        .and(body_string(json!({ "enabled": true }).to_string()))
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
        .set_thread_enabled(true, Some("network-0001"))
        .await?;
    // Invalidated by the write: this must be a fresh request, the 2nd of the `expect(2)` above.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn update_thread_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/thread"))
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
    client
        .update_thread(Some(true), None, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn regenerate_thread_credentials_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(session_cookie())
        .and(body_string("\"\""))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({"meta": {"code": 200}, "data": {"network": {}}}).to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .regenerate_thread_credentials(Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
