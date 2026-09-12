//! P3 settings-shaped module suite: the read-only (`GET`) half of `eero-api`'s `DnsAPI`,
//! `SecurityAPI`, `SqmAPI` and `SettingsAPI`, all against a local `wiremock` server per
//! the crate's testing conventions.
//!
//! One test per method pins the exact verb, path and session cookie, and asserts the returned
//! [`rusteero::envelope::Envelope`] is byte-identical to its fixture. A dedicated
//! `dns_security_and_sqm_settings_all_hit_the_same_network_path` test proves
//! `get_dns_settings`/`get_security_settings`/`get_sqm_settings` all fetch the exact same
//! `networks/{network_id}` resource — the full network object, not a settings sub-resource —
//! exactly as `eero-api src/eero/api/dns.py:36-57`, `security.py:36-57` and `sqm.py:36-57` do. A
//! final error-path test proves a `404` on an unknown network id maps to
//! `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::dns::DnsApi;
use rusteero::endpoints::security::SecurityApi;
use rusteero::endpoints::settings::SettingsApi;
use rusteero::endpoints::sqm::SqmApi;
use rusteero::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`DnsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn dns_api(mock: &MockEero) -> DnsApi {
    DnsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SecurityApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SqmApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn sqm_api(mock: &MockEero) -> SqmApi {
    SqmApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SettingsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn settings_api(mock: &MockEero) -> SettingsApi {
    SettingsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_dns_settings =====================

#[tokio::test]
async fn get_dns_settings_hits_networks_id_and_returns_the_fixture_envelope() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.get_dns_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_security_settings =====================

#[tokio::test]
async fn get_security_settings_hits_networks_id_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.get_security_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_sqm_settings =====================

#[tokio::test]
async fn get_sqm_settings_hits_networks_id_and_returns_the_fixture_envelope() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api.get_sqm_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_settings =====================

#[tokio::test]
async fn get_settings_hits_the_dedicated_settings_subresource_not_the_full_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A single mock registered against the dedicated `networks/{id}/settings` path (not the bare
    // `networks/{id}` path the other three methods in this file hit): if `get_settings` ever
    // regressed to fetching the full network object instead, this mock would never match and the
    // call below would fail with a wiremock "no match" error rather than a response.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = settings_api(&mock);
    let env = api.get_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== shared-path proof =====================

#[tokio::test]
async fn dns_security_and_sqm_settings_all_hit_the_same_network_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A single mock registered against the shared `GET /2.2/networks/{id}` path, expected
    // exactly 3 times: if any of `get_dns_settings`/`get_security_settings`/`get_sqm_settings`
    // rendered a different path (e.g. a `.../dns`, `.../security` or `.../sqm` sub-resource),
    // this mock would never match that call and the request would fail with a wiremock "no
    // match" error rather than a response — proving all three hit the exact same wire endpoint
    // as `NetworksApi::get_network`, the full network object.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(3)
        .mount(&mock.server)
        .await;

    let dns_env = dns_api(&mock).get_dns_settings("network-0001").await?;
    let security_env = security_api(&mock)
        .get_security_settings("network-0001")
        .await?;
    let sqm_env = sqm_api(&mock).get_sqm_settings("network-0001").await?;

    let expected = fixture_json("network.json");
    assert_eq!(dns_env.into_value(), expected);
    assert_eq!(security_env.into_value(), expected);
    assert_eq!(sqm_env.into_value(), expected);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_dns_settings_with_unknown_id_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let err = api
        .get_dns_settings("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
