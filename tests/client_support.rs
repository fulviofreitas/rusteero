//! `Client` integration suite for the `SupportAPI` domain.

mod common;

use serde_json::json;
use tracing::Level;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, capture_logs, fixture, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_support_resolves_the_explicit_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"email": "support@example.com"} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_support(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

/// A cached network that publishes its `support` link on 2.3 still leads to a read on 2.2: 2.3
/// answers with a `contacts` array where 2.2 answers with a flat object.
#[tokio::test]
async fn get_support_pins_to_2_2_when_the_cached_network_publishes_a_2_3_link() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"email": "support@example.com"} });
    mount_network_publishing_support(&mock, "/2.3/networks/network-0001/support").await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let env = client.get_support(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

/// Mounts `GET /2.2/networks/network-0001` answering an envelope that publishes `support_link`.
async fn mount_network_publishing_support(mock: &MockEero, support_link: &str) {
    let network = json!({
        "meta": { "code": 200 },
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": { "support": support_link },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

// ===================== request_support =====================

#[tokio::test]
async fn request_support_posts_the_body_verbatim_and_warns_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let request = json!({ "subject": "slow wi-fi", "nested": { "anything": [1, 2, 3] } });
    let response = json!({ "meta": {"code": 200}, "data": {"ticket": "t-1"} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(request.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_string(response.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let (_guard, logs) = capture_logs();
    let env = client
        .request_support(request, Some("network-0001"), None)
        .await?;

    assert_eq!(env.as_value(), &response);
    assert_eq!(
        logs.count(Level::WARN, "Issuing write (request support for network)"),
        1,
        "got {:?}",
        logs.warnings()
    );
    Ok(())
}

/// The write invalidates nothing: every cached bucket survives it.
#[tokio::test]
async fn request_support_leaves_every_cache_bucket_alone() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for (get_path, body) in [
        ("/2.2/networks/network-0001", fixture("network.json")),
        ("/2.2/networks/network-0001/eeros", fixture("eeros.json")),
        (
            "/2.2/networks/network-0001/devices",
            fixture("devices.json"),
        ),
        (
            "/2.2/networks/network-0001/profiles",
            fixture("profiles.json"),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(get_path))
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&mock.server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let network = Some("network-0001");
    client.get_network(network, false).await?;
    client.get_eeros(network, false).await?;
    client.get_devices(network, false, None, None).await?;
    client.get_profiles(network, false).await?;

    client
        .request_support(json!({ "subject": "x" }), network, None)
        .await?;

    // Every read below is served from the cache: each GET above was expected exactly once.
    client.get_network(network, false).await?;
    client.get_eeros(network, false).await?;
    client.get_devices(network, false, None, None).await?;
    client.get_profiles(network, false).await?;
    Ok(())
}

/// A caller's `parent` wins over the cached network envelope, which in turn wins over the
/// template; unlike `get_support`, the write follows the published link on whatever version it
/// names.
#[tokio::test]
async fn request_support_prefers_the_callers_parent_then_the_cached_network() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_network_publishing_support(&mock, "/2.4/networks/network-0001/support").await;
    for (post_path, times) in [
        ("/2.2/networks/network-0001/support", 0),
        ("/2.4/networks/network-0001/support", 1),
        ("/2.2/networks/network-0001/caller-support", 1),
    ] {
        Mock::given(method("POST"))
            .and(path(post_path))
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(times)
            .mount(&mock.server)
            .await;
    }

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;

    // No parent: the cached network's published link.
    client
        .request_support(json!({ "subject": "x" }), Some("network-0001"), None)
        .await?;
    // A parent: its own link, whatever the cache holds.
    let parent = json!({"resources": {"support": "/2.2/networks/network-0001/caller-support"}});
    client
        .request_support(
            json!({ "subject": "x" }),
            Some("network-0001"),
            Some(&parent),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn request_support_without_a_network_fails_without_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .request_support(json!({ "subject": "x" }), None, None)
        .await
        .expect_err("no network id and no preferred network must fail");
    assert!(matches!(err, Error::MissingNetworkId), "got {err:?}");
    Ok(())
}
