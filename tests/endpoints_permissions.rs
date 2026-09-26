//! `PermissionsApi` suite (`src/endpoints/permissions.rs`, new in v8.0.0) against a local
//! `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::permissions::PermissionsApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn permissions_api(mock: &MockEero) -> PermissionsApi {
    PermissionsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

#[tokio::test]
async fn get_permissions_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"permissions": {"network.admin_invites": true}, "role": "OWNER"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/permissions"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = permissions_api(&mock)
        .get_permissions("network-0001", None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_permissions_prefers_the_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"permissions": {}, "role": "MEMBER"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/permissions"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/permissions"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.4/networks/network-0001"});
    let env = permissions_api(&mock)
        .get_permissions("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
