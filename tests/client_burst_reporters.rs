//! `Client` integration suite for `BurstReportersApi`'s one wrapper, `Client::create_burst_reporter`
//! (`client.py:1534-1581` at `v8.4.1`): the `""` body, the silent default path, parent
//! precedence, the device-list-only cache invalidation, and network resolution without
//! auto-discovery.

mod common;

use serde_json::{Value, json};
use tracing::Level;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie, user_token_header};
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

/// The envelope the live API returned for a burst request.
fn next_burst_envelope() -> Value {
    json!({ "meta": { "code": 200 }, "data": { "next_burst": "2026-10-17T10:00:00Z" } })
}

/// A network envelope that publishes `burst_reporters` at API 2.4.
fn network_with_burst_link() -> Value {
    json!({
        "meta": {"code": 200},
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "burst_reporters": "/2.4/networks/network-0001/burst_reporters",
            },
        },
    })
}

/// Mounts the authenticated `""` POST at `post_path`, expecting exactly `calls` requests.
async fn mount_burst_post(mock: &MockEero, post_path: &str, calls: u64) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(next_burst_envelope()))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// Mounts a trap on `post_path` that must never be reached.
async fn mount_burst_trap(mock: &MockEero, post_path: &str) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

/// Mounts `GET <get_path>` answering `body`, authenticated, expecting `calls` requests.
async fn mount_get(mock: &MockEero, get_path: &str, body: String, calls: u64) {
    Mock::given(method("GET"))
        .and(path(get_path))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn create_burst_reporter_posts_the_empty_json_string() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_burst_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let client = client(&mock).await;
    let env = client
        .create_burst_reporter(Some("network-0001"), None)
        .await?;

    assert_eq!(env.data()["next_burst"], "2026-10-17T10:00:00Z");
    assert_eq!(env.into_value(), next_burst_envelope());
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_logs_no_uncharacterised_warning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_burst_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;

    let client = client(&mock).await;
    let (_guard, logs) = common::capture_logs();
    let env = client
        .create_burst_reporter(Some("network-0001"), None)
        .await?;

    assert_eq!(env.into_value(), next_burst_envelope());
    // Exactly the one POST: no discovery GET and no parent GET (`assert_called_once`).
    assert_eq!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        1
    );
    // Neither the uncharacterised-write warning nor the `reporter_data` deprecation notice.
    assert_eq!(logs.count(Level::WARN, "not been fully characterised"), 0);
    assert!(
        logs.warnings().is_empty(),
        "unexpected warnings: {:?}",
        logs.warnings()
    );
    // Positive control: the capture does see such a warning when one is emitted.
    rusteero::links::warn_uncharacterised_write("probe");
    assert_eq!(logs.count(Level::WARN, "not been fully characterised"), 1);
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_uses_the_cached_networks_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        network_with_burst_link().to_string(),
        1,
    )
    .await;
    mount_burst_post(&mock, "/2.4/networks/network-0001/burst_reporters", 1).await;
    mount_burst_trap(&mock, "/2.2/networks/network-0001/burst_reporters").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .create_burst_reporter(Some("network-0001"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_prefers_the_callers_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        network_with_burst_link().to_string(),
        1,
    )
    .await;
    // The caller's link is a third, distinct path: neither the cached link nor the template.
    mount_burst_post(&mock, "/2.3/networks/network-0001/burst_reporters", 1).await;
    mount_burst_trap(&mock, "/2.4/networks/network-0001/burst_reporters").await;
    mount_burst_trap(&mock, "/2.2/networks/network-0001/burst_reporters").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let parent =
        json!({"resources": {"burst_reporters": "/2.3/networks/network-0001/burst_reporters"}});
    client
        .create_burst_reporter(Some("network-0001"), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_invalidates_only_the_device_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_burst_post(&mock, "/2.2/networks/network-0001/burst_reporters", 1).await;
    // The device list of the burst's network is refetched; everything else stays cached.
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/device-0001",
        fixture("device.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0002/devices",
        fixture("devices.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        1,
    )
    .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .get_device("device-0001", Some("network-0001"), false)
        .await?;
    client
        .get_devices(Some("network-0002"), false, None, None)
        .await?;
    client.get_network(Some("network-0001"), false).await?;

    client
        .create_burst_reporter(Some("network-0001"), None)
        .await?;

    // The follow-up list read reaches the API (the mock's second request) ...
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    // ... while the single device, the other network's list and the network stay cached.
    client
        .get_device("device-0001", Some("network-0001"), false)
        .await?;
    client
        .get_devices(Some("network-0002"), false, None, None)
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_failure_leaves_the_device_list_cached() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // The same authenticated `""` POST `mount_burst_post` pins, answered with a 500.
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        1,
    )
    .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    let err = client
        .create_burst_reporter(Some("network-0001"), None)
        .await
        .expect_err("a server error must propagate");
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err:?}");

    // Still served from the cache: the mock allows only the first list request.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_burst_reporter_resolves_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;
    mount_burst_trap(&mock, "/2.2/networks/network-0001/burst_reporters").await;

    let client = client(&mock).await;
    let err = client
        .create_burst_reporter(None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}
