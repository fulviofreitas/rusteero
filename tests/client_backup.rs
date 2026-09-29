//! `Client` integration suite for `BackupApi` (rewritten for v8.0.4): network-id resolution and
//! `set_backup_internet`'s `Net{nid}` cache invalidation.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
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
async fn get_backup_internet_resolves_the_network_id_and_returns_the_envelope() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "enabled": false } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backupinternet"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_backup_internet(Some("network-0001")).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn set_backup_internet_invalidates_the_network_cache_entry() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backupinternet"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_backup_internet(true, Some("network-0001"))
        .await?;
    let second = client.get_network(Some("network-0001"), false).await?;

    // Two `GET /2.2/networks/network-0001` calls (`.expect(2)` above) prove the second
    // `get_network` re-hit the wire instead of serving the pre-write cached entry.
    assert_eq!(second.as_value(), &fixture_json("network.json"));
    Ok(())
}

#[tokio::test]
async fn get_cellular_backup_usage_and_events_hit_their_own_paths() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let usage = json!({ "meta": { "code": 200 }, "data": { "bytes_used": 1024 } });
    let events = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/cellular_backup_usage"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(usage.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/cellular_backup_events"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(events.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let usage_env = client
        .get_cellular_backup_usage(Some("network-0001"))
        .await?;
    let events_env = client
        .get_cellular_backup_events(Some("network-0001"))
        .await?;

    assert_eq!(usage_env.into_value(), usage);
    assert_eq!(events_env.into_value(), events);
    Ok(())
}
