//! HTTP integration tests for `DnsApi` (`src/endpoints/dns.rs`) against a local `wiremock`
//! server per the crate's testing conventions.
//!
//! One test per read method pins the exact verb, path and session cookie, and asserts the
//! returned [`rusteero::envelope::Envelope`] is byte-identical to its fixture. A final
//! error-path test proves a `404` on an unknown network id maps to
//! `Error::Api { status: 404, .. }`.
//!
//! Every mutating test pins the exact verb, path, session cookie and request body (`body_json`,
//! an exact deep-equality match, not a subset match) with a wiremock `.expect(n)` call count
//! verified at server-drop time.
//!
//! - `set_custom_dns_truncates_three_servers_to_two` proves the silent 2-entry cap
//!   (`dns.py:114-116`).
//! - `set_dns_mode_invalid_mode_is_validation_error_with_no_requests` and
//!   `set_dns_mode_custom_without_servers_is_validation_error_with_no_requests` prove an
//!   unrecognised (or under-supplied `"custom"`) mode never reaches the network — no `Mock` is
//!   registered at all, so a regression that *did* send a request would hit wiremock's default
//!   404-for-unmatched-request behaviour and fail the `Error::Validation` assertion, and the
//!   explicit `received_requests().await` check makes that assertion airtight.
//!
//! **Judgment call (test-file split, 2026-09-25)**: `dns_security_and_sqm_settings_all_hit_
//! the_same_network_path` exercises `DnsApi`, `SecurityApi` and `SqmApi` together — it proves
//! all three read the exact same `GET /2.2/networks/{id}` resource `NetworksApi::get_network`
//! does. It cannot be assigned to a single domain without duplicating or fragmenting the
//! assertion, so it is kept here (alphabetically first of the three domains it touches); a
//! change to `SecurityApi` or `SqmApi` that breaks this test's expectations will need to touch
//! this file too, alongside `endpoints_security.rs`/`endpoints_sqm.rs`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::dns::DnsApi;
use rusteero::endpoints::security::SecurityApi;
use rusteero::endpoints::sqm::SqmApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`DnsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn dns_api(mock: &MockEero) -> DnsApi {
    DnsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SecurityApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]) — used only by the shared-path proof
/// below; see this file's module docs for why.
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SqmApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]) — used only by the shared-path proof
/// below; see this file's module docs for why.
fn sqm_api(mock: &MockEero) -> SqmApi {
    SqmApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
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

// =================================== DnsApi::set_dns_caching ===================================

#[tokio::test]
async fn set_dns_caching_puts_settings_with_dns_caching() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "dns_caching": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_caching("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== DnsApi::set_custom_dns ===================================

#[tokio::test]
async fn set_custom_dns_truncates_three_servers_to_two() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exactly 2 entries expected — the 3rd ("9.9.9.9") must never appear in the body. See this
    // phase's report for the red/green capture proving this assertion actually catches a
    // regression (temporarily expecting all 3 entries fails; restoring the 2-entry expectation
    // below passes again).
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": ["1.1.1.1", "1.0.0.1"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api
        .set_custom_dns("network-0001", &["1.1.1.1", "1.0.0.1", "9.9.9.9"])
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_with_two_servers_sends_both_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": ["8.8.8.8", "8.8.4.4"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api
        .set_custom_dns("network-0001", &["8.8.8.8", "8.8.4.4"])
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================== DnsApi::clear_custom_dns ==================================

#[tokio::test]
async fn clear_custom_dns_puts_settings_with_an_empty_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": [] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.clear_custom_dns("network-0001").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ===================================== DnsApi::set_dns_mode =====================================

#[tokio::test]
async fn set_dns_mode_cloudflare_sends_cloudflare_resolvers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": ["1.1.1.1", "1.0.0.1"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_mode("network-0001", "cloudflare", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_google_sends_google_resolvers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": ["8.8.8.8", "8.8.4.4"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_mode("network-0001", "google", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_opendns_sends_opendns_resolvers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "custom_dns": ["208.67.222.222", "208.67.220.220"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_mode("network-0001", "opendns", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_custom_sends_provided_servers_truncated_to_two() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "custom_dns": ["9.9.9.9", "149.112.112.112"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let servers = ["9.9.9.9", "149.112.112.112", "1.1.1.1"];
    let env = api
        .set_dns_mode("network-0001", "custom", Some(&servers))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_auto_sends_empty_custom_dns() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "custom_dns": [] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_mode("network-0001", "auto", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_invalid_mode_is_validation_error_with_no_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // No `Mock` registered: if `set_dns_mode` ever reached the network for an unrecognised
    // mode, the request would hit wiremock's default 404-for-unmatched-request response and
    // surface as `Error::Api { status: 404, .. }`, not `Error::Validation` — failing the
    // assertion below.
    let api = dns_api(&mock);
    let err = api
        .set_dns_mode("network-0001", "not-a-real-mode", None)
        .await
        .expect_err("an unrecognised mode must be rejected before any request");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "mode"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_custom_without_servers_is_validation_error_with_no_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Mirrors Python's `mode == "custom" and custom_servers` truthiness check (`dns.py:170`):
    // `"custom"` with no servers supplied is just as invalid as an unrecognised mode string.
    let api = dns_api(&mock);
    let err = api
        .set_dns_mode("network-0001", "custom", None)
        .await
        .expect_err("\"custom\" with no servers must be rejected before any request");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "mode"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

// =================================== DnsApi::set_ipv6_dns ===================================

#[tokio::test]
async fn set_ipv6_dns_puts_settings_with_ipv6_upstream_only() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body: if `set_ipv6_dns` ever also sent `ipv6_downstream` (copying
    // `SecurityApi::set_ipv6`'s fan-out by mistake), this mock would never match.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "ipv6_upstream": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_ipv6_dns("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}
