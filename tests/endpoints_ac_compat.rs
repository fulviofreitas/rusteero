//! HTTP integration test for `ACCompatApi` (`src/endpoints/ac_compat.rs`).
//!
//! `ACCompatApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_ac_compat.py`). No real MACs, serials, IPs or names appear in any fixture
//! here.
//!
//! Per the crate's testing conventions, the test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body served via `into_value()` — the raw wire payload is the contract, never a reshaped
//! view of it.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ac_compat::ACCompatApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

// ===================== get_ac_compat =====================

#[tokio::test]
async fn get_ac_compat_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"compatible": true, "devices": []},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ac_compat"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ACCompatApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_ac_compat("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}
