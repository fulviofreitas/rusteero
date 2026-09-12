//! P5 settings-mutation suite: the write halves of `NetworksApi`, `DnsApi`, `SecurityApi` and
//! `SqmApi` (`src/endpoints/networks.rs`, `dns.rs`, `security.rs`, `sqm.rs`), all against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! None of these four modules has a committed fixture for its write responses, so every
//! response body here is a small, obviously-synthetic `{"meta": …, "data": …}` value built
//! inline with `serde_json::json!` — no real MACs, serials, IPs or names — matching the
//! convention `tests/endpoints_backup_group.rs` already established for fixture-less modules.
//!
//! Every test pins the exact verb, path, session cookie and request body (`body_json`, an exact
//! deep-equality match, not a subset match) with a wiremock `.expect(n)` call count verified at
//! server-drop time. A handful of tests carry extra weight beyond "one test per method":
//!
//! - `set_custom_dns_truncates_three_servers_to_two` proves the silent 2-entry cap
//!   (`dns.py:114-116`).
//! - `set_dns_mode_invalid_mode_is_validation_error_with_no_requests` and
//!   `set_dns_mode_custom_without_servers_is_validation_error_with_no_requests` prove an
//!   unrecognised (or under-supplied `"custom"`) mode never reaches the network — no `Mock` is
//!   registered at all, so a regression that *did* send a request would hit wiremock's default
//!   404-for-unmatched-request behaviour and fail the `Error::Validation` assertion, and the
//!   explicit `received_requests().await` check makes that assertion airtight (the same idiom
//!   `tests/client.rs`'s `Error::MissingNetworkId` test and `tests/auth.rs`'s
//!   `auth_api_set_session_token_empty_is_validation_error_with_no_requests` use).
//! - `set_ipv6_puts_settings_with_both_upstream_and_downstream` proves `SecurityApi::set_ipv6`
//!   fans one bool out to *two* wire keys (`security.py:185-188`); `set_ipv6_dns_puts_settings_
//!   with_ipv6_upstream_only` proves the DNS-layer sibling does not.
//! - `configure_security_with_no_fields_is_validation_error_with_no_requests` mirrors the same
//!   zero-request proof for `SecurityApi::configure_security` (`security.py:272-274`).
//! - `sqm_enabled_is_flat_while_bandwidth_configure_and_auto_all_nest_under_sqm` asserts, in one
//!   test, that `set_sqm_enabled` sends a **flat** `{"sqm": bool}` while `set_sqm_bandwidth`,
//!   `configure_sqm` and `set_sqm_auto` all send a **nested** `{"sqm": {...}}` — the
//!   flat/nested split `sqm.py`'s three `TODO: Verify` comments (`sqm.py:128`, `:173`, `:197`)
//!   leave unconfirmed upstream; see each method's own doc comment in `src/endpoints/sqm.rs`.
//!   `PARITY.md` rows for `set_sqm_bandwidth`/`configure_sqm`/`set_sqm_auto` must read "ported —
//!   shape unverified upstream", not "ported".
//!
//! Two tests that could not fail have already shipped in this project; this suite adds no third
//! — the DNS-truncation and SQM flat/nested tests were each red/green-verified by hand (expected
//! body deliberately broken, observed panic, restored, observed pass) before being committed
//! here; see this phase's report for the captured output.

mod common;

use std::sync::Arc;

use rusteero::endpoints::dns::DnsApi;
use rusteero::endpoints::networks::NetworksApi;
use rusteero::endpoints::security::SecurityApi;
use rusteero::endpoints::sqm::SqmApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

/// Builds a [`NetworksApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn networks_api(mock: &MockEero) -> NetworksApi {
    NetworksApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`DnsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn dns_api(mock: &MockEero) -> DnsApi {
    DnsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SecurityApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SqmApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn sqm_api(mock: &MockEero) -> SqmApi {
    SqmApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
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
    // above for why this, plus the explicit `received_requests()` check below, proves zero
    // requests were made.
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

// ================================== SqmApi::set_sqm_enabled ==================================

#[tokio::test]
async fn set_sqm_enabled_puts_settings_with_flat_sqm_bool() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match FLAT body `{"sqm": true}` — not `{"sqm": {"enabled": true}}`. See
    // `sqm_enabled_is_flat_while_bandwidth_configure_and_auto_all_nest_under_sqm` below for the
    // explicit flat-vs-nested contrast across all four SQM setters.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "sqm": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api.set_sqm_enabled("network-0001", true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================ SqmApi::set_sqm_bandwidth ================================

#[tokio::test]
async fn set_sqm_bandwidth_with_both_limits_nests_them_under_sqm() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({
            "sqm": { "enabled": true, "upload_bandwidth": 100, "download_bandwidth": 50 },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api
        .set_sqm_bandwidth("network-0001", Some(100), Some(50))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_sqm_bandwidth_with_no_limits_sends_enabled_only() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body proves the bandwidth keys are omitted entirely, not sent as `null`, when
    // neither argument is supplied.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "sqm": { "enabled": true } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api.set_sqm_bandwidth("network-0001", None, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== SqmApi::configure_sqm ===================================

#[tokio::test]
async fn configure_sqm_enabled_with_limits_nests_them_under_sqm() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({
            "sqm": { "enabled": true, "upload_bandwidth": 200, "download_bandwidth": 20 },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api
        .configure_sqm("network-0001", true, Some(200), Some(20))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn configure_sqm_disabled_omits_bandwidth_keys_even_if_provided() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // `enabled: false` must drop the bandwidth keys entirely, even though both are supplied
    // (`sqm.py:165-169`'s `if enabled:` gate) — an exact-match body on `{"sqm": {"enabled":
    // false}}` alone proves it.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "sqm": { "enabled": false } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api
        .configure_sqm("network-0001", false, Some(200), Some(20))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== SqmApi::set_sqm_auto ===================================

#[tokio::test]
async fn set_sqm_auto_puts_settings_with_fixed_auto_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "sqm": { "enabled": true, "mode": "auto" } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api.set_sqm_auto("network-0001").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================= SQM flat-vs-nested, asserted together =============================

#[tokio::test]
async fn sqm_enabled_is_flat_while_bandwidth_configure_and_auto_all_nest_under_sqm()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Four separate mocks, each matching one setter's exact body shape. If `set_sqm_enabled`
    // ever nested its bool (or any of the other three ever flattened theirs), the corresponding
    // mock would never match and this test would fail at server-drop verification — see this
    // phase's report for the red/green capture proving that.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "sqm": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({
            "sqm": { "enabled": true, "upload_bandwidth": 10 },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "sqm": { "enabled": false } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "sqm": { "enabled": true, "mode": "auto" } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    api.set_sqm_enabled("network-0001", true).await?;
    api.set_sqm_bandwidth("network-0001", Some(10), None)
        .await?;
    api.configure_sqm("network-0001", false, None, None).await?;
    api.set_sqm_auto("network-0001").await?;
    Ok(())
}
