//! HTTP integration test for `ACCompatApi` (`src/endpoints/ac_compat.rs`).
//!
//! `ACCompatApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_ac_compat.py`). No real MACs, serials, IPs or names appear in any fixture
//! here.

mod common;

use std::sync::Arc;

use rusteero::endpoints::ac_compat::ACCompatApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn ac_compat_api(mock: &MockEero) -> ACCompatApi {
    ACCompatApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_ac_compat =====================

#[tokio::test]
async fn get_ac_compat_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"compatible": true, "devices": []},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ac_compat"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = ac_compat_api(&mock)
        .get_ac_compat("network-0001", None)
        .await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_ac_compat_prefers_the_parents_ac_compat_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"compatible": false} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ac_compat"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-ac-compat"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"ac_compat": "/2.2/networks/network-0001/custom-ac-compat"}});
    let env = ac_compat_api(&mock)
        .get_ac_compat("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
