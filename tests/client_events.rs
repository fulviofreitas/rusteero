//! `Client` integration suite for `EventsApi` (new in v8.0.0): network-id resolution and the
//! `+net` parent passthrough every wrapper in this domain uses.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::events::GetChannelUtilizationOptions;

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
async fn get_app_events_passes_the_cached_network_envelope_as_parent() -> anyhow::Result<()> {
    // The cached network envelope below carries a `/2.3` self `url` — a fresh cache read after
    // `get_network` must make `get_app_events` prefer that self-url over the `/2.2` template.
    let mock = MockEero::start().await;
    let network_body = json!({
        "meta": {"code": 200},
        "data": {"id": "network-0001", "url": "/2.3/networks/network-0001"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let events_body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/app_events"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(events_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let env = client
        .get_app_events(None, None, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), events_body);
    Ok(())
}

#[tokio::test]
async fn get_app_events_falls_back_to_the_template_with_an_empty_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/app_events"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_app_events(None, None, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_network_scan_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "scan": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/network_scan"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_network_scan(Some("network-0001")).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_forwards_start_end_and_options() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .and(session_cookie())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("granularity", "30"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let options = GetChannelUtilizationOptions {
        granularity: Some(30),
        ..GetChannelUtilizationOptions::default()
    };
    let env = client
        .get_channel_utilization("s", "e", &options, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
