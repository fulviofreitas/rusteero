//! HTTP integration tests for `SqmApi` (`src/endpoints/sqm.rs`) against a local `wiremock`
//! server per the crate's testing conventions.
//!
//! One test pins the exact verb, path and session cookie for `get_sqm_settings`, and asserts the
//! returned [`rusteero::envelope::Envelope`] is byte-identical to its fixture. See
//! `endpoints_dns.rs`'s `dns_security_and_sqm_settings_all_hit_the_same_network_path` for the
//! cross-domain proof that this method, `DnsApi::get_dns_settings` and
//! `SecurityApi::get_security_settings` all hit the exact same `networks/{network_id}` resource
//! (a judgment call recorded there).
//!
//! `sqm_enabled_is_flat_while_bandwidth_configure_and_auto_all_nest_under_sqm` asserts, in one
//! test, that `set_sqm_enabled` sends a **flat** `{"sqm": bool}` while `set_sqm_bandwidth`,
//! `configure_sqm` and `set_sqm_auto` all send a **nested** `{"sqm": {...}}` — the flat/nested
//! split `sqm.py`'s three `TODO: Verify` comments (`sqm.py:128`, `:173`, `:197`) leave
//! unconfirmed upstream; see each method's own doc comment in `src/endpoints/sqm.rs`.
//! `PARITY.md` rows for `set_sqm_bandwidth`/`configure_sqm`/`set_sqm_auto` must read "ported —
//! shape unverified upstream", not "ported". This test, plus
//! `set_sqm_enabled_puts_settings_with_flat_sqm_bool` below, were each red/green-verified by
//! hand (expected body deliberately broken, observed panic, restored, observed pass); see this
//! crate's task report for the captured output.

mod common;

use std::sync::Arc;

use rusteero::endpoints::sqm::SqmApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`SqmApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn sqm_api(mock: &MockEero) -> SqmApi {
    SqmApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
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
