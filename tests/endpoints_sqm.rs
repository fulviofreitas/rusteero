//! HTTP integration tests for `SqmApi` (`src/endpoints/sqm.rs`), v8.0.4, against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! See `endpoints_dns.rs`'s `dns_security_and_sqm_settings_all_hit_the_same_network_path` for the
//! cross-domain proof that `get_sqm_settings` (with no `parent`), `DnsApi::get_dns_settings` and
//! `SecurityApi::get_security_settings` all hit the exact same `networks/{network_id}` resource.

mod common;

use std::sync::Arc;

use rusteero::endpoints::sqm::SqmApi;
use serde_json::json;
use wiremock::matchers::{body_string, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("sqm_settings.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = sqm_api(&mock);
    let env = api.get_sqm_settings("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("sqm_settings.json"));
    Ok(())
}

#[tokio::test]
async fn get_sqm_settings_prefers_a_parent_supplied_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("sqm_settings.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = sqm_api(&mock);
    api.get_sqm_settings("network-0001", Some(&parent)).await?;
    Ok(())
}

// ================================== SqmApi::set_sqm ==================================

#[tokio::test]
async fn set_sqm_sends_the_query_param_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for (enabled, expected) in [(true, "true"), (false, "false")] {
        Mock::given(method("PUT"))
            .and(path("/2.2/networks/network-0001/settings"))
            .and(query_param("sqm", expected))
            .and(body_string(""))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(1)
            .mount(&mock.server)
            .await;

        let api = sqm_api(&mock);
        let env = api.set_sqm("network-0001", enabled, None).await?;
        assert_eq!(env.into_value(), ok_envelope());
    }
    Ok(())
}

#[tokio::test]
async fn set_sqm_prefers_the_parents_published_settings_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(query_param("sqm", "true"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = sqm_api(&mock);
    api.set_sqm("network-0001", true, Some(&parent)).await?;
    Ok(())
}
