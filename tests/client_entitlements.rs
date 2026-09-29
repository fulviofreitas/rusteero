//! `Client` integration suite for the `EntitlementsAPI` domain (new in v8.0.0).

mod common;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_entitlement_features_resolves_the_preferred_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"features": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/entitlements/networks/network-0001/features"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_preferred_network("network-0001");
    let env = client.get_entitlement_features(None).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

#[tokio::test]
async fn get_upsell_features_resolves_the_explicit_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"upsell_features": []} });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/entitlements/networks/network-0001/upsell_features",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_upsell_features(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

#[tokio::test]
async fn get_model_capabilities_sends_the_networkid_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"models": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/eero_models/capabilities"))
        .and(query_param("networkId", "network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_model_capabilities(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

#[tokio::test]
async fn get_premium_customer_has_no_network_id_argument() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"is_premium": false} });
    Mock::given(method("GET"))
        .and(path("/2.2/premium/customer"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_premium_customer().await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}
