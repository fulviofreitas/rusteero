//! P3 `NetworksApi` suite: the read-only (`GET`) half of `eero-api`'s `NetworksAPI`, plus
//! `get_account`, all against a local `wiremock` server per the crate's testing conventions.
//!
//! One test per method pins the exact verb, path and session cookie, and asserts the returned
//! [`rusteero::envelope::Envelope`] is byte-identical to its fixture. A dedicated
//! `get_premium_status_hits_the_same_path_as_get_network` test proves the two methods share one
//! wire endpoint, exactly as `eero-api src/eero/api/networks.py:159-180` does. A final error-path
//! test proves a `404` on an unknown network id maps to `Error::NotFound { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::networks::NetworksApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`NetworksApi`] pointed at `mock`, wrapping a [`Transport`] already authenticated
/// with [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn networks_api(mock: &MockEero) -> NetworksApi {
    NetworksApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
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
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ============================= NetworksApi::set_guest_network =============================

#[tokio::test]
async fn set_guest_network_puts_guestnetwork_with_full_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(body_json(
            json!({ "enabled": true, "name": "Guest", "password": "hunter2" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_guest_network("network-0001", true, Some("Guest"), Some("hunter2"))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_guest_network_with_only_enabled_omits_name_and_password() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // An exact `body_json` match on a body with only "enabled": if `name`/`password` were ever
    // sent as `null` keys instead of omitted entirely, this mock would never match.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_guest_network("network-0001", false, None, None)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================ NetworksApi::run_speed_test ================================

#[tokio::test]
async fn run_speed_test_posts_empty_body_to_speedtest_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.run_speed_test("network-0001").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================ NetworksApi::reboot_network ================================

#[tokio::test]
async fn reboot_network_posts_empty_body_to_reboot_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reboot"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.reboot_network("network-0001").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =============================== NetworksApi::set_network_name ===============================

#[tokio::test]
async fn set_network_name_puts_settings_with_name() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "name": "My Network" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.set_network_name("network-0001", "My Network").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}
