//! `Client` integration suite for the `UpdatesAPI` domain.

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};
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
async fn get_updates_resolves_the_explicit_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"available": true} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_updates(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

#[tokio::test]
async fn apply_update_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"id":"network-0001"}})),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.apply_update(Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== set_preferred_update_hour =====================

const HOUR_PATH: &str = "/2.2/networks/network-0001/updates/preferred_update_hour";

fn ok_envelope() -> serde_json::Value {
    json!({"meta": {"code": 200}, "data": {}})
}

async fn assert_no_requests(mock: &MockEero) {
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
}

#[tokio::test]
async fn set_preferred_update_hour_posts_to_the_explicit_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(json!({"preferred_update_hour": 3})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .set_preferred_update_hour(3, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_forwards_both_boundary_hours() -> anyhow::Result<()> {
    for hour in [0, 23] {
        let mock = MockEero::start().await;
        Mock::given(method("POST"))
            .and(path(HOUR_PATH))
            .and(session_cookie())
            .and(user_token_header())
            .and(body_json(json!({"preferred_update_hour": hour})))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(1)
            .mount(&mock.server)
            .await;

        let client = client(&mock).await;
        client
            .set_preferred_update_hour(hour, Some("network-0001"))
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_uses_the_preferred_network_without_discovery()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0002/updates/preferred_update_hour",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({"preferred_update_hour": 7})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_preferred_network("network-0002");
    client.set_preferred_update_hour(7, None).await?;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_rejects_invalid_hours_before_resolving_the_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    // No explicit and no preferred network: resolution would fail with `MissingNetworkId`, which
    // the validation error must pre-empt.
    let client = client(&mock).await;
    for hour in [-1, 24] {
        let err = client
            .set_preferred_update_hour(hour, None)
            .await
            .expect_err("an out-of-range hour must be rejected first");
        assert!(
            matches!(
                err,
                Error::Validation { ref field, ref message, .. }
                    if field == "hour" && message == "must be an int from 0 to 23"
            ),
            "hour {hour}: got {err:?}"
        );
    }
    let err = client
        .set_preferred_update_hour(24, Some("network-0001"))
        .await
        .expect_err("an out-of-range hour is rejected before any request");
    assert!(matches!(err, Error::Validation { .. }), "got {err:?}");

    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_without_a_network_is_missing_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .set_preferred_update_hour(3, None)
        .await
        .expect_err("no explicit and no preferred network");
    assert!(matches!(err, Error::MissingNetworkId), "got {err:?}");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_invalidates_only_the_written_networks_cache_entry()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"id":"network-0001"}})),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    // The other network's entry must survive the write: fetched once, then served from the cache.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0002"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"id":"network-0002"}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_network(Some("network-0002"), false).await?;
    client
        .set_preferred_update_hour(3, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    client.get_network(Some("network-0002"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_failed_write_keeps_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"id":"network-0001"}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let err = client
        .set_preferred_update_hour(3, Some("network-0001"))
        .await
        .expect_err("a 500 must surface");
    assert!(matches!(err, Error::Api { status: 500, .. }), "got {err:?}");

    // Still served from the cache: the GET mock above allows exactly one request.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
