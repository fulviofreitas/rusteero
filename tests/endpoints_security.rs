//! HTTP integration tests for `SecurityApi` (`src/endpoints/security.rs`), v8.0.4, against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! See `endpoints_dns.rs`'s `dns_security_and_sqm_settings_all_hit_the_same_network_path` for the
//! cross-domain proof that `get_security_settings` (with no `parent`), `DnsApi::get_dns_settings`
//! and `SqmApi::get_sqm_settings` all hit the exact same `networks/{network_id}` resource.

mod common;

use std::sync::Arc;

use rusteero::endpoints::security::SecurityApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds a [`SecurityApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

/// The parent envelope fixture publishing every sub-resource link this file's writers can
/// prefer (`resources.settings`/`mlo_mode`/`fast_transition`/`passpoint`/`proxied_nodes`), all
/// under the `/2.4` version prefix so a test can prove the link, not the template, was used.
fn parent_with_links() -> serde_json::Value {
    fixture_json("dns_network_with_settings_link.json")
}

// ===================== get_security_settings =====================

#[tokio::test]
async fn get_security_settings_hits_networks_id_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.get_security_settings("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

#[tokio::test]
async fn get_security_settings_prefers_a_parent_supplied_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // The parent's own `url` (self_url preference, not a `resources.<name>` link) must win.
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = parent_with_links();
    let api = security_api(&mock);
    api.get_security_settings("network-0001", Some(&parent))
        .await?;
    Ok(())
}

// ==================================== SecurityApi::set_wpa3 ====================================

#[tokio::test]
async fn set_wpa3_puts_settings_with_wpa3() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_wpa3("network-0001", true, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_wpa3_uses_the_default_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    api.set_wpa3("network-0001", true, None).await?;
    Ok(())
}

#[tokio::test]
async fn set_wpa3_prefers_the_parents_published_settings_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = parent_with_links();
    let api = security_api(&mock);
    api.set_wpa3("network-0001", true, Some(&parent)).await?;
    Ok(())
}

// ============================== SecurityApi::set_band_steering ==============================

#[tokio::test]
async fn set_band_steering_puts_settings_with_band_steering() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({ "band_steering": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_band_steering("network-0001", false, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =================================== SecurityApi::set_upnp ===================================

#[tokio::test]
async fn set_upnp_puts_settings_with_upnp() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({ "upnp": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_upnp("network-0001", true, None).await?;
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
        .and(body_json(
            json!({ "ipv6_upstream": true, "ipv6_downstream": true }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.set_ipv6("network-0001", true, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================== SecurityApi::configure_security ==============================

#[tokio::test]
async fn configure_security_with_all_fields_merges_them_into_one_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "wpa3": true,
            "band_steering": false,
            "upnp": true,
            "ipv6_upstream": false,
            "ipv6_downstream": false,
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
            None,
        )
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn configure_security_with_no_fields_is_validation_error_with_no_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
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

// =================================== SecurityApi::set_mlo_mode ===================================

#[tokio::test]
async fn set_mlo_mode_sends_json_for_every_declared_mode() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for candidate in ["disabled", "single", "multi"] {
        Mock::given(method("PUT"))
            .and(path("/2.2/networks/network-0001/mlo_mode"))
            .and(body_json(json!({ "mlo_mode": candidate })))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let api = security_api(&mock);
    for candidate in ["disabled", "single", "multi"] {
        api.set_mlo_mode("network-0001", candidate, None).await?;
    }
    Ok(())
}

#[tokio::test]
async fn set_mlo_mode_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/mlo_mode"))
        .and(body_json(json!({ "mlo_mode": "single" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = parent_with_links();
    let api = security_api(&mock);
    api.set_mlo_mode("network-0001", "single", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_mlo_mode_rejects_an_invalid_mode_with_no_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = security_api(&mock);
    let err = api
        .set_mlo_mode("network-0001", "DISABLED", None)
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

// ============================== SecurityApi::{get,set}_fast_transition ==============================

#[tokio::test]
async fn get_fast_transition_hits_the_sub_resource() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "fast_transition": true } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/fast_transition"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    let env = api.get_fast_transition("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn set_fast_transition_sends_json_for_both_values() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for enabled in [true, false] {
        Mock::given(method("PUT"))
            .and(path("/2.2/networks/network-0001/fast_transition"))
            .and(body_json(json!({ "fast_transition": enabled })))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let api = security_api(&mock);
    for enabled in [true, false] {
        api.set_fast_transition("network-0001", enabled, None)
            .await?;
    }
    Ok(())
}

// ============================== SecurityApi::set_passpoint_enabled ==============================

#[tokio::test]
async fn set_passpoint_enabled_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/passpoint/enabled"))
        .and(body_json(json!({ "enabled": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    api.set_passpoint_enabled("network-0001", true, None)
        .await?;
    Ok(())
}

// ============================== SecurityApi::set_proxied_nodes ==============================

#[tokio::test]
async fn set_proxied_nodes_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/proxied_nodes"))
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = security_api(&mock);
    api.set_proxied_nodes("network-0001", false, None).await?;
    Ok(())
}
