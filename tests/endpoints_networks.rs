//! `NetworksApi` suite at v8.0.4: every method in `src/endpoints/networks.rs`, plus
//! `get_account`, against a local `wiremock` server per the crate's testing conventions.
//!
//! One test per method pins the exact verb, path, body/query and both credential headers
//! (`user_token_header()` + `session_cookie()`); every `Resource` with a `link` gets one test
//! that a `parent` carrying the link wins over the bare-id template, and one that a bare id falls
//! back to the template.

mod common;

use std::sync::Arc;

use rusteero::endpoints::networks::NetworksApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_string, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

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
        .and(user_token_header())
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
async fn get_network_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_network("network-0001", None).await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

#[tokio::test]
async fn get_network_prefers_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // The parent's own `url` field wins over the bare-id template entirely: a mock on the
    // template path is registered with `.expect(0)` so a regression that ignores `parent` fails
    // loudly instead of silently matching the wrong mock.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.4/networks/network-0001"});
    let api = networks_api(&mock);
    let env = api.get_network("network-0001", Some(&parent)).await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_premium_status =====================

#[tokio::test]
async fn get_premium_status_hits_the_same_path_as_get_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A single mock registered against the shared `GET /2.2/networks/{id}` path: if
    // `get_premium_status` rendered a different path than `get_network`, this mock would never
    // match — proving the two methods hit the exact same wire endpoint.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_premium_status("network-0001", None).await?;
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
        .and(user_token_header())
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
        .get_network("does-not-exist", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ================================ NetworksApi::reboot_network ================================

#[tokio::test]
async fn reboot_network_posts_the_empty_json_string_to_the_reboot_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reboot"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.reboot_network("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn reboot_network_prefers_parents_reboot_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reboot"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/custom-reboot"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"reboot": "/2.2/networks/network-0001/custom-reboot"}});
    let api = networks_api(&mock);
    let env = api.reboot_network("network-0001", Some(&parent)).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================ NetworksApi::run_speed_test ================================

#[tokio::test]
async fn run_speed_test_posts_the_empty_json_string_to_the_speedtest_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.run_speed_test("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ================================ NetworksApi::get_speed_tests ================================

#[tokio::test]
async fn get_speed_tests_sends_every_supplied_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(query_param("limit", "5"))
        .and(query_param("startTime", "2026-01-01T00:00:00Z"))
        .and(query_param("endTime", "2026-01-02T00:00:00Z"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":[]})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .get_speed_tests(
            "network-0001",
            Some(5),
            Some("2026-01-01T00:00:00Z"),
            Some("2026-01-02T00:00:00Z"),
            None,
        )
        .await?;
    assert_eq!(env.data(), &json!([]));
    Ok(())
}

#[tokio::test]
async fn get_speed_tests_omits_unsupplied_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":[]})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .get_speed_tests("network-0001", None, None, None, None)
        .await?;
    assert_eq!(env.data(), &json!([]));
    Ok(())
}

// =============================== NetworksApi::set_network_name ===============================

#[tokio::test]
async fn set_network_name_sends_a_form_encoded_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("name=New-Network-Name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_network_name("network-0001", "New-Network-Name", None)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================= NetworksApi::set_network_password =============================

#[tokio::test]
async fn set_network_password_sends_a_form_encoded_payload() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("password=hunter2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_network_password("network-0001", "hunter2", None)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================ NetworksApi::clear_network_password ============================

#[tokio::test]
async fn clear_network_password_deletes_the_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.clear_network_password("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// =============================== NetworksApi::get_guest_network ===============================

#[tokio::test]
async fn get_guest_network_returns_the_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"enabled": true} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_guest_network("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ============================= NetworksApi::set_guest_network =============================

#[tokio::test]
async fn set_guest_network_sends_form_encoded_payload_with_name() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("enabled=true&name=Guest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_guest_network("network-0001", true, Some("Guest"), None)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_guest_network_omits_name_when_not_given() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // An exact `body_string` match on a body carrying only "enabled": if `name` were ever sent
    // as an empty value instead of omitted entirely, this mock would never match.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(body_string("enabled=false"))
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

#[tokio::test]
async fn set_guest_network_prefers_parents_guestnetwork_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/custom-guest"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"guestnetwork": "/2.2/networks/network-0001/custom-guest"}});
    let api = networks_api(&mock);
    let env = api
        .set_guest_network("network-0001", true, None, Some(&parent))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================= NetworksApi::set_guest_password =============================

#[tokio::test]
async fn set_guest_password_uses_the_literal_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("password=guestpass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api
        .set_guest_password("network-0001", "guestpass", None)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_guest_password_prefers_the_guest_parents_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/guestnetwork/custom-password",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    // `parent` here is the *guest network's own* envelope (the result of `get_guest_network`),
    // not the network's — its `resources.password` link is what wins.
    let guest_parent = json!({
        "resources": {"password": "/2.2/networks/network-0001/guestnetwork/custom-password"},
    });
    let api = networks_api(&mock);
    let env = api
        .set_guest_password("network-0001", "guestpass", Some(&guest_parent))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================ NetworksApi::clear_guest_password ============================

#[tokio::test]
async fn clear_guest_password_deletes_the_guest_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.clear_guest_password("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}
