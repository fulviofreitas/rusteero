//! HTTP integration tests for `UpdatesApi` (`src/endpoints/updates.rs`).
//!
//! `UpdatesApi` has no fixture under `tests/fixtures/`, so every response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_updates.py`). No real MACs, serials, IPs or names appear in any fixture here.

mod common;

use std::sync::Arc;

use rusteero::endpoints::updates::UpdatesApi;
use serde_json::json;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn updates_api(mock: &MockEero) -> UpdatesApi {
    UpdatesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_updates =====================

#[tokio::test]
async fn get_updates_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"available": true, "current_version": "6.15.0"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = updates_api(&mock).get_updates("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_updates_prefers_the_parents_updates_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"available": false} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"updates": "/2.2/networks/network-0001/custom-updates"}});
    let env = updates_api(&mock)
        .get_updates("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== apply_update =====================

#[tokio::test]
async fn apply_update_posts_the_empty_json_string_to_the_updates_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "meta": { "code": 200 }, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = updates_api(&mock)
        .apply_update("network-0001", None)
        .await?;
    assert_eq!(
        env.into_value(),
        json!({ "meta": { "code": 200 }, "data": {} })
    );
    Ok(())
}

#[tokio::test]
async fn apply_update_prefers_the_parents_updates_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/updates"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/custom-updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "meta": { "code": 200 }, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"updates": "/2.2/networks/network-0001/custom-updates"}});
    let env = updates_api(&mock)
        .apply_update("network-0001", Some(&parent))
        .await?;
    assert_eq!(
        env.into_value(),
        json!({ "meta": { "code": 200 }, "data": {} })
    );
    Ok(())
}
