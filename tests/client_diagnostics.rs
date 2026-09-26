//! `Client` integration suite for the `diagnostics` domain
//! (`eero-api src/eero/client.py:1246-1273`).

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_diagnostics_resolves_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_diagnostics(None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}

#[tokio::test]
async fn get_diagnostics_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "{\"meta\":{},\"data\":{\"resources\":{\"diagnostics\":\"/2.3/networks/network-0001/diagnostics\"}}}",
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"meta\":{},\"data\":{}}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_diagnostics(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn run_diagnostics_forwards_the_supplied_device_and_symptom() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(body_json(
            json!({ "device": "dev-0001", "symptom": "slow" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .run_diagnostics(Some("network-0001"), Some("dev-0001"), Some("slow"))
        .await?;
    Ok(())
}
