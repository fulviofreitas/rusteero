//! HTTP integration tests for `BlacklistApi` (`src/endpoints/blacklist.rs`) at `v8.0.4`.
//!
//! `BlacklistApi` has no committed fixture file, so response bodies are small, obviously-synthetic
//! values built inline with `serde_json::json!` — no real MACs, serials, IPs or names.

mod common;

use std::sync::Arc;

use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds a [`BlacklistApi`] pointed at `mock`, wrapping a `Transport` already authenticated
/// with [`TEST_TOKEN`].
fn blacklist_api(mock: &MockEero) -> BlacklistApi {
    BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_blacklist =====================

#[tokio::test]
async fn get_blacklist_hits_blacklist_path_and_returns_the_served_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": [
            {"mac": "aa:bb:cc:00:00:01", "device_id": "aabbcc000001"},
            {"mac": "aa:bb:cc:00:00:02", "device_id": "aabbcc000002"},
        ],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    let env = api.get_blacklist("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_blacklist_prefers_parents_device_blacklist_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"device_blacklist": "/2.4/networks/network-0001/blacklist"}});
    let api = blacklist_api(&mock);
    api.get_blacklist("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== add_to_blacklist =====================

#[tokio::test]
async fn add_to_blacklist_sends_a_form_encoded_mac_not_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("mac=aa%3Abb%3Acc%3A00%3A00%3A01"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    api.add_to_blacklist("network-0001", "aa:bb:cc:00:00:01", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn add_to_blacklist_prefers_parents_device_blacklist_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.4/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"device_blacklist": "/2.4/networks/network-0001/blacklist"}});
    let api = blacklist_api(&mock);
    api.add_to_blacklist("network-0001", "aa:bb:cc:00:00:01", Some(&parent))
        .await?;
    Ok(())
}

// ===================== remove_from_blacklist =====================

#[tokio::test]
async fn remove_from_blacklist_hits_the_right_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/aabbcc000001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    api.remove_from_blacklist("network-0001", "aabbcc000001", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_rejects_a_traversal_identifier_before_any_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    let err = api
        .remove_from_blacklist("network-0001", "a/../../account", None)
        .await
        .expect_err("a traversal-shaped id must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));

    let err = api
        .remove_from_blacklist("network-0001", "abc?x=1", None)
        .await
        .expect_err("a query-carrying id must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}
