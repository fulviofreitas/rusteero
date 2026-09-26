//! HTTP integration tests for `ThreadApi` (`src/endpoints/thread.rs`), v8.0.4, against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! `ThreadApi` has no dedicated fixture for its writes: response bodies are small, obviously
//! synthetic `{"meta": …, "data": …}` values built inline with `serde_json::json!`. No real
//! MACs, serials, IPs or names appear anywhere in this file.

mod common;

use std::sync::Arc;

use rusteero::endpoints::thread::ThreadApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds a [`ThreadApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn thread_api(mock: &MockEero) -> ThreadApi {
    ThreadApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_thread =====================

#[tokio::test]
async fn get_thread_hits_the_thread_link_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("thread_status.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = thread_api(&mock);
    let env = api.get_thread("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("thread_status.json"));
    Ok(())
}

#[tokio::test]
async fn get_thread_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/thread"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("thread_status.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = thread_api(&mock);
    api.get_thread("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== set_thread_enabled =====================

#[tokio::test]
async fn set_thread_enabled_sends_json_payload_to_the_literal_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(json!({ "enabled": true }).to_string()))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = thread_api(&mock);
    let env = api.set_thread_enabled("network-0001", true, None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn set_thread_enabled_ignores_a_supplied_parent_and_still_uses_the_literal_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Even though this parent publishes a `thread` link on `/2.4`, the write must still target
    // the literal `/2.2` path — `parent` is accepted but never consulted for Thread writes.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/thread"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "meta": {"code": 200}, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = thread_api(&mock);
    api.set_thread_enabled("network-0001", true, Some(&parent))
        .await?;
    Ok(())
}

// ===================== update_thread =====================

#[tokio::test]
async fn update_thread_sends_only_the_supplied_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(body_string(json!({ "thread_enable": true }).to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "meta": {"code": 200}, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = thread_api(&mock);
    api.update_thread("network-0001", Some(true), None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn update_thread_requires_at_least_one_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = thread_api(&mock);
    let err = api
        .update_thread("network-0001", None, None, None)
        .await
        .expect_err("both fields omitted must be rejected before any request");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "thread"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

// ===================== regenerate_thread_credentials =====================

#[tokio::test]
async fn regenerate_thread_credentials_posts_the_empty_json_string() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "network": {} } });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/thread"))
        .and(body_string("\"\""))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = thread_api(&mock);
    let env = api
        .regenerate_thread_credentials("network-0001", None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
