//! `Client` integration suite for `EerosApi`'s pass-throughs and cache invalidation.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

// ===================== One representative mutation =====================

#[tokio::test]
async fn set_led_reaches_the_eero_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_led("eero-0001", true, Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("eero.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================

#[tokio::test]
async fn reboot_eero_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .reboot_eero("eero-0001", Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== A deliberate improvement over Python =====================

#[tokio::test]
async fn set_led_brightness_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    // Divergence from eero-api (rust-port-plan.md §3.8, improvement (b)): Python's
    // `set_led_brightness` invalidates nothing at all.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .set_led_brightness("eero-0001", 42, Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}
