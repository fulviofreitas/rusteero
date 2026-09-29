//! `EntitlementsApi` suite (`src/endpoints/entitlements.rs`, new in v8.0.0) against a local
//! `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::entitlements::EntitlementsApi;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn entitlements_api(mock: &MockEero) -> EntitlementsApi {
    EntitlementsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_features =====================

#[tokio::test]
async fn get_features_accepts_a_bare_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"features": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/entitlements/networks/network-0001/features"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = entitlements_api(&mock).get_features("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_features_accepts_an_absolute_entitlements_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"features": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/entitlements/networks/network-0001/features"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    // `resource_url`'s absolute-URL branch substitutes `id_or_url` for the `{id}`-terminated
    // prefix and then appends the template's own suffix (`/features`) — so the value passed here
    // is the resource's URL *up to* the id, not the final `/features` endpoint itself.
    let absolute = format!("{}/2.2/entitlements/networks/network-0001", mock.uri());
    let env = entitlements_api(&mock).get_features(&absolute).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_upsell_features =====================

#[tokio::test]
async fn get_upsell_features_returns_the_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"upsell_features": []} });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/entitlements/networks/network-0001/upsell_features",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = entitlements_api(&mock)
        .get_upsell_features("network-0001")
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_model_capabilities =====================

#[tokio::test]
async fn get_model_capabilities_sends_the_network_id_as_a_raw_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"models": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/eero_models/capabilities"))
        .and(query_param("networkId", "network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = entitlements_api(&mock)
        .get_model_capabilities("network-0001")
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_premium_customer =====================

#[tokio::test]
async fn get_premium_customer_returns_the_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"is_premium": true} });
    Mock::given(method("GET"))
        .and(path("/2.2/premium/customer"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = entitlements_api(&mock).get_premium_customer().await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
