//! HTTP integration tests for `SecurityApi` (`src/endpoints/security.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! One test pins the exact verb, path and session cookie for `get_security_settings`, and
//! asserts the returned [`rusteero::envelope::Envelope`] is byte-identical to its fixture. See
//! `endpoints_dns.rs`'s `dns_security_and_sqm_settings_all_hit_the_same_network_path` for the
//! cross-domain proof that this method, `DnsApi::get_dns_settings` and `SqmApi::get_sqm_settings`
//! all hit the exact same `networks/{network_id}` resource (a judgment call recorded there).
//!
//! Every mutating test pins the exact verb, path, session cookie and request body (`body_json`,
//! an exact deep-equality match, not a subset match) with a wiremock `.expect(n)` call count
//! verified at server-drop time.
//!
//! - `set_ipv6_puts_settings_with_both_upstream_and_downstream` proves `SecurityApi::set_ipv6`
//!   fans one bool out to *two* wire keys (`security.py:185-188`).
//! - `configure_security_with_no_fields_is_validation_error_with_no_requests` proves the
//!   zero-request guard for `SecurityApi::configure_security` (`security.py:272-274`).

mod common;

use std::sync::Arc;

use rusteero::endpoints::security::SecurityApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`SecurityApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
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

// ==================================== SecurityApi::set_wpa3 ====================================

#[tokio::test]
async fn set_wpa3_puts_settings_with_wpa3() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_wpa3("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================== SecurityApi::set_band_steering ==============================

#[tokio::test]
async fn set_band_steering_puts_settings_with_band_steering() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "band_steering": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_band_steering("network-0001", false).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== SecurityApi::set_upnp ===================================

#[tokio::test]
async fn set_upnp_puts_settings_with_upnp() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "upnp": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_upnp("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== SecurityApi::set_ipv6 ===================================

#[tokio::test]
async fn set_ipv6_puts_settings_with_both_upstream_and_downstream() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body carrying BOTH keys: if `set_ipv6` ever sent only one of
    // `ipv6_upstream`/`ipv6_downstream`, this mock would never match.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "ipv6_upstream": true, "ipv6_downstream": true }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_ipv6("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================== SecurityApi::set_thread ==================================

#[tokio::test]
async fn set_thread_puts_settings_with_thread() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "thread": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_thread("network-0001", false).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================== SecurityApi::configure_security ==============================

#[tokio::test]
async fn configure_security_with_all_fields_merges_them_into_one_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({
            "wpa3": true,
            "band_steering": false,
            "upnp": true,
            "ipv6_upstream": false,
            "ipv6_downstream": false,
            "thread": true,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api
        .configure_security(
            "network-0001",
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
        )
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn configure_security_with_no_fields_is_validation_error_with_no_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // No `Mock` registered — see `set_dns_mode_invalid_mode_is_validation_error_with_no_requests`
    // in `endpoints_dns.rs` for why this, plus the explicit `received_requests()` check below,
    // proves zero requests were made.
    let api = security_api(&mock);
    let err = api
        .configure_security("network-0001", None, None, None, None, None)
        .await
        .expect_err("an empty call must be rejected before any request");
    assert!(matches!(err, Error::Validation { .. }));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}
