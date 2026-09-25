//! HTTP integration test for `UpdatesApi` (`src/endpoints/updates.rs`).
//!
//! `UpdatesApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_updates.py`). No real MACs, serials, IPs or names appear in any fixture here.
//!
//! Per the crate's testing conventions, the test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body served via `into_value()` — the raw wire payload is the contract, never a reshaped
//! view of it.

mod common;

use std::sync::Arc;

use rusteero::endpoints::updates::UpdatesApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

// ===================== get_updates =====================

#[tokio::test]
async fn get_updates_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"available": true, "current_version": "6.15.0"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = UpdatesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_updates("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}
