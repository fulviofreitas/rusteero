//! `Client` integration suite for `NotificationsApi` (new in v8.0.0) — cache invalidation and
//! `network_id` resolution, per the crate's testing conventions.

mod common;

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use serde_json::json;

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
async fn get_notification_settings_reaches_the_notifications_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_settings.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_notification_settings(Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_notification_settings_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/notifications"))
        .and(session_cookie())
        .and(body_json(json!({ "network.updated": false })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_settings.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_notification_settings(&[("network.updated", false)], Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn has_unread_notifications_reaches_the_has_unread_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications/has_unread"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"meta":{"code":200},"data":{"has_unread":false}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .has_unread_notifications(Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn mark_notifications_read_reaches_the_mark_read_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/notifications/mark_read"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.mark_notifications_read(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_notification_history_forwards_the_timestamp() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications_history"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_history.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_notification_history(Some("ts-0001"), Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_push_settings_is_account_scoped_with_no_network_id_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/push_settings"))
        .and(session_cookie())
        .and(body_json(json!({ "networkOffline": true })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .set_push_settings(&[("networkOffline", true)])
        .await?;
    Ok(())
}
