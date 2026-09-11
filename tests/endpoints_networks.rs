//! P3 `NetworksApi` suite: the read-only (`GET`) half of `eero-api`'s `NetworksAPI`, plus
//! `get_account`, all against a local `wiremock` server per `.claude/rules/testing.md`.
//!
//! One test per method pins the exact verb, path and session cookie, and asserts the returned
//! [`rusteero::envelope::Envelope`] is byte-identical to its fixture. A dedicated
//! `get_premium_status_hits_the_same_path_as_get_network` test proves the two methods share one
//! wire endpoint, exactly as `eero-api src/eero/api/networks.py:159-180` does. A final error-path
//! test proves a `404` on an unknown network id maps to `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::networks::NetworksApi;
use rusteero::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`NetworksApi`] pointed at `mock`, wrapping a [`Transport`] already authenticated
/// with [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn networks_api(mock: &MockEero) -> NetworksApi {
    NetworksApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_networks =====================

#[tokio::test]
async fn get_networks_hits_get_networks_and_returns_the_fixture_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_networks().await?;
    assert_eq!(env.into_value(), fixture_json("networks.json"));
    Ok(())
}

// ===================== get_network =====================

#[tokio::test]
async fn get_network_hits_networks_id_and_returns_the_fixture_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_network("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_premium_status =====================

#[tokio::test]
async fn get_premium_status_hits_the_same_path_as_get_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A single mock registered against the shared `GET /2.2/networks/{id}` path: if
    // `get_premium_status` rendered a different path than `get_network`, this mock would never
    // match and the call below would fail with a wiremock "no match" error rather than a
    // response — proving the two methods hit the exact same wire endpoint.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_premium_status("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_account =====================

#[tokio::test]
async fn get_account_hits_account_and_returns_the_fixture_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_account().await?;
    assert_eq!(env.into_value(), fixture_json("account.json"));
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_network_with_unknown_id_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let err = api
        .get_network("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
