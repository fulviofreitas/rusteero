//! HTTP integration tests for `BlacklistApi` (`src/endpoints/blacklist.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! `BlacklistApi` has no committed fixture file, so `get_blacklist`'s response body is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_blacklist.py`) use — no real MACs, serials, IPs or names.
//!
//! `add_to_blacklist`/`remove_from_blacklist` pin the exact verb, path and (for the `POST`) JSON
//! body via `body_json`, with `.expect(n)` call counts verified at server-drop time.

mod common;

use std::sync::Arc;

use rusteero::endpoints::blacklist::BlacklistApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

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
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    let env = api.get_blacklist("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== add_to_blacklist =====================

#[tokio::test]
async fn add_to_blacklist_body_is_exactly_mac() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .and(body_json(json!({ "mac": "aa:bb:cc:00:00:01" })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    api.add_to_blacklist("network-0001", "aa:bb:cc:00:00:01")
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
    api.remove_from_blacklist("network-0001", "aabbcc000001")
        .await?;

    Ok(())
}
