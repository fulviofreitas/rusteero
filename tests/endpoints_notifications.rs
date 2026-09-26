//! HTTP integration tests for `NotificationsApi` (new in v8.0.0), per the crate's testing
//! conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::notifications::NotificationsApi;
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

fn notifications_api(mock: &MockEero) -> NotificationsApi {
    NotificationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_settings =====================

#[tokio::test]
async fn get_settings_hits_the_literal_notifications_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_settings.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    let env = api.get_settings("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("notification_settings.json"));
    Ok(())
}

#[tokio::test]
async fn get_settings_prefers_the_parents_own_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/notifications"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_settings.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    let parent = json!({"url": "/2.4/networks/network-0001"});
    api.get_settings("ignored-network", Some(&parent)).await?;
    Ok(())
}

// ===================== set_settings =====================

#[tokio::test]
async fn set_settings_sends_the_supplied_pairs_verbatim() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/notifications"))
        .and(session_cookie())
        .and(body_json(json!({
            "network.updated": false,
            "permissions.updates": true
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_settings.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    api.set_settings(
        "network-0001",
        &[("network.updated", false), ("permissions.updates", true)],
        None,
    )
    .await?;
    Ok(())
}

// ===================== has_unread =====================

#[tokio::test]
async fn has_unread_hits_the_has_unread_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications/has_unread"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"meta":{"code":200},"data":{"has_unread":true}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    let env = api.has_unread("network-0001", None).await?;
    assert_eq!(
        env.data()
            .get("has_unread")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    Ok(())
}

// ===================== mark_read =====================

#[tokio::test]
async fn mark_read_posts_the_literal_empty_json_string_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/notifications/mark_read"))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    api.mark_read("network-0001", None).await?;
    Ok(())
}

// ===================== get_history =====================

#[tokio::test]
async fn get_history_sends_the_timestamp_query_param_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/notifications_history"))
        .and(query_param("timestamp", "ts-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("notification_history.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    api.get_history("network-0001", Some("ts-0001"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_history_omits_the_timestamp_query_param_when_not_given() -> anyhow::Result<()> {
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

    let api = notifications_api(&mock);
    let env = api.get_history("network-0001", None, None).await?;
    assert_eq!(env.into_value(), fixture_json("notification_history.json"));
    Ok(())
}

// ===================== set_push_settings =====================

#[tokio::test]
async fn set_push_settings_puts_to_the_account_level_path_with_camelcase_keys() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/push_settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({
            "networkOffline": true,
            "nodeOffline": false
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = notifications_api(&mock);
    api.set_push_settings(&[("networkOffline", true), ("nodeOffline", false)])
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_push_settings_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = NotificationsApi::new(Arc::new(mock.transport_anonymous()));
    let err = api
        .set_push_settings(&[("networkOffline", true)])
        .await
        .expect_err("no session must fail before any request");
    assert!(err.is_auth_error());
    Ok(())
}
