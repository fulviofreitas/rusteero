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
use wiremock::matchers::{
    body_json, body_string, header, method, path, query_param, query_param_is_missing,
};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

fn networks_api(mock: &MockEero) -> NetworksApi {
    NetworksApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

/// Asserts that the mock server has not received a single request.
async fn assert_no_requests(mock: &MockEero) {
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
}

/// Asserts that `err` is `Error::Validation` for `field` with the fixed non-empty-string message.
fn assert_non_empty_string_error(err: &Error, field: &str) {
    let Error::Validation {
        field: got_field,
        message,
        ..
    } = err
    else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(got_field, field);
    assert_eq!(message, "must be a non-empty string");
    assert_eq!(
        err.to_string(),
        format!("Validation error for '{field}': must be a non-empty string")
    );
    assert!(!err.is_auth_error());
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

#[tokio::test]
async fn get_premium_status_prefers_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Same self-url resolution as `get_network` (`NetworksApi::network_own_url`): the parent's
    // own `url` field wins over the bare-id template.
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
    let env = api
        .get_premium_status("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}

// ===================== get_account =====================

// `NetworksApi::get_account` is deprecated in favour of `AccountApi::get_account`; this pins that
// the kept method still sends the same request.
#[allow(deprecated)]
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
        .and(user_token_header())
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
        .and(user_token_header())
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

#[tokio::test]
async fn run_speed_test_prefers_parents_speedtest_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"speedtest": "/2.3/networks/network-0001/speedtest"}});
    let api = networks_api(&mock);
    let env = api.run_speed_test("network-0001", Some(&parent)).await?;
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
        .and(user_token_header())
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
        .and(query_param_is_missing("limit"))
        .and(query_param_is_missing("startTime"))
        .and(query_param_is_missing("endTime"))
        .and(user_token_header())
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

#[tokio::test]
async fn get_speed_tests_prefers_parents_speedtest_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":[]})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"speedtest": "/2.3/networks/network-0001/speedtest"}});
    let api = networks_api(&mock);
    let env = api
        .get_speed_tests("network-0001", None, None, None, Some(&parent))
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

#[tokio::test]
async fn set_network_name_prefers_parents_settings_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"settings": "/2.3/networks/network-0001/settings"}});
    let api = networks_api(&mock);
    let env = api
        .set_network_name("network-0001", "New-Network-Name", Some(&parent))
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
        .and(user_token_header())
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

#[tokio::test]
async fn set_network_password_prefers_parents_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/password"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"password": "/2.3/networks/network-0001/password"}});
    let api = networks_api(&mock);
    let env = api
        .set_network_password("network-0001", "hunter2", Some(&parent))
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.clear_network_password("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn clear_network_password_prefers_parents_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/password"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/2.3/networks/network-0001/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"password": "/2.3/networks/network-0001/password"}});
    let api = networks_api(&mock);
    let env = api
        .clear_network_password("network-0001", Some(&parent))
        .await?;
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.get_guest_network("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_guest_network_prefers_parents_guestnetwork_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"enabled": true} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"guestnetwork": "/2.3/networks/network-0001/guestnetwork"}});
    let api = networks_api(&mock);
    let env = api.get_guest_network("network-0001", Some(&parent)).await?;
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
        .and(user_token_header())
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
        .and(user_token_header())
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
        .and(user_token_header())
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
        .and(user_token_header())
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = networks_api(&mock);
    let env = api.clear_guest_password("network-0001", None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn clear_guest_password_prefers_the_guest_parents_password_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/guestnetwork/custom-password",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    // Same as `set_guest_password`: `parent` here is the *guest network's own* envelope.
    let guest_parent = json!({
        "resources": {"password": "/2.2/networks/network-0001/guestnetwork/custom-password"},
    });
    let api = networks_api(&mock);
    let env = api
        .clear_guest_password("network-0001", Some(&guest_parent))
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ============================= Wi-Fi password validation =============================

#[tokio::test]
async fn set_network_password_rejects_an_empty_password_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = networks_api(&mock)
        .set_network_password("network-0001", "", None)
        .await
        .expect_err("an empty password must be rejected before any request");

    assert_non_empty_string_error(&err, "password");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_guest_password_rejects_an_empty_password_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = networks_api(&mock)
        .set_guest_password("network-0001", "", None)
        .await
        .expect_err("an empty password must be rejected before any request");

    assert_non_empty_string_error(&err, "password");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn password_validation_precedes_the_auth_gate() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = NetworksApi::new(Arc::new(mock.transport_anonymous()));

    let err = api
        .set_network_password("network-0001", "", None)
        .await
        .expect_err("an empty password wins over a missing session");
    assert_non_empty_string_error(&err, "password");

    let err = api
        .set_guest_password("network-0001", "", None)
        .await
        .expect_err("an empty password wins over a missing session");
    assert_non_empty_string_error(&err, "password");

    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn password_validation_precedes_url_resolution_and_the_warning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (_guard, logs) = common::capture_logs();
    let api = networks_api(&mock);

    // An empty network id would fail URL resolution with field `id_or_url`; the password check
    // must come first.
    let err = api
        .set_network_password("", "", None)
        .await
        .expect_err("an empty password wins over an unresolvable network id");
    assert_non_empty_string_error(&err, "password");

    let err = api
        .set_guest_password("", "", None)
        .await
        .expect_err("an empty password wins over an unresolvable network id");
    assert_non_empty_string_error(&err, "password");

    assert!(
        logs.warnings().is_empty(),
        "an invalid call must log no uncharacterised-write warning: {:?}",
        logs.warnings()
    );
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_network_password_still_warns_once_for_a_valid_password() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let (_guard, logs) = common::capture_logs();

    networks_api(&mock)
        .set_network_password("network-0001", "hunter2", None)
        .await?;

    assert_eq!(
        logs.count(
            tracing::Level::WARN,
            "Issuing write (set network password for network)"
        ),
        1,
        "got {:?}",
        logs.warnings()
    );
    assert!(
        logs.warnings().iter().all(|line| !line.contains("hunter2")),
        "the password must never be logged: {:?}",
        logs.warnings()
    );
    Ok(())
}

#[tokio::test]
async fn set_guest_password_logs_no_uncharacterised_write_warning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let (_guard, logs) = common::capture_logs();

    networks_api(&mock)
        .set_guest_password("network-0001", "guestpass", None)
        .await?;

    assert!(
        logs.warnings().is_empty(),
        "the guest password write is live-verified and must not warn: {:?}",
        logs.warnings()
    );
    Ok(())
}

#[tokio::test]
async fn password_setters_forward_padded_and_literal_none_values_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // (password, form body): padding is preserved (never trimmed), a whitespace-only password is
    // accepted because only its length is tested, and the text "None" is just text.
    let cases = [
        ("  preserve spaces  ", "password=++preserve+spaces++"),
        ("None", "password=None"),
        (" ", "password=+"),
    ];
    for resource in ["password", "guestnetwork/password"] {
        for (_, body) in cases {
            Mock::given(method("PUT"))
                .and(path(format!("/2.2/networks/network-0001/{resource}")))
                .and(session_cookie())
                .and(user_token_header())
                .and(header("content-type", "application/x-www-form-urlencoded"))
                .and(body_string(body))
                .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
                .expect(1)
                .mount(&mock.server)
                .await;
        }
    }

    let api = networks_api(&mock);
    for (password, _) in cases {
        api.set_network_password("network-0001", password, None)
            .await?;
        api.set_guest_password("network-0001", password, None)
            .await?;
    }
    Ok(())
}

// ============================= Network and guest-network name validation =============================

#[tokio::test]
async fn set_network_name_rejects_an_empty_name_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = networks_api(&mock)
        .set_network_name("network-0001", "", None)
        .await
        .expect_err("an empty name must be rejected before any request");

    assert_non_empty_string_error(&err, "name");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_guest_network_rejects_an_empty_supplied_name_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = networks_api(&mock)
        .set_guest_network("network-0001", true, Some(""), None)
        .await
        .expect_err("a supplied empty name must be rejected before any request");

    assert_non_empty_string_error(&err, "name");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn name_validation_precedes_auth_and_url_resolution() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (_guard, logs) = common::capture_logs();

    // A missing session: the name check must win over the credential gate.
    let anonymous = NetworksApi::new(Arc::new(mock.transport_anonymous()));
    let err = anonymous
        .set_network_name("network-0001", "", None)
        .await
        .expect_err("an empty name wins over a missing session");
    assert_non_empty_string_error(&err, "name");
    let err = anonymous
        .set_guest_network("network-0001", true, Some(""), None)
        .await
        .expect_err("an empty name wins over a missing session");
    assert_non_empty_string_error(&err, "name");

    // An unresolvable network id: the name check must win over URL resolution (`id_or_url`).
    let api = networks_api(&mock);
    let err = api
        .set_network_name("", "", None)
        .await
        .expect_err("an empty name wins over an unresolvable network id");
    assert_non_empty_string_error(&err, "name");
    let err = api
        .set_guest_network("", true, Some(""), None)
        .await
        .expect_err("an empty name wins over an unresolvable network id");
    assert_non_empty_string_error(&err, "name");

    assert!(
        logs.warnings().is_empty(),
        "an invalid call must log no uncharacterised-write warning: {:?}",
        logs.warnings()
    );
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_network_name_still_warns_once_for_a_valid_name() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let (_guard, logs) = common::capture_logs();

    networks_api(&mock)
        .set_network_name("network-0001", "My Network", None)
        .await?;

    assert_eq!(
        logs.count(
            tracing::Level::WARN,
            "Issuing write (set network name for network)"
        ),
        1,
        "got {:?}",
        logs.warnings()
    );
    Ok(())
}

#[tokio::test]
async fn set_guest_network_logs_no_uncharacterised_write_warning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let (_guard, logs) = common::capture_logs();

    networks_api(&mock)
        .set_guest_network("network-0001", true, Some("Guest"), None)
        .await?;

    assert!(
        logs.warnings().is_empty(),
        "the guest network write is live-verified and must not warn: {:?}",
        logs.warnings()
    );
    Ok(())
}

#[tokio::test]
async fn name_setters_forward_padded_and_literal_none_values_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // (name, `set_network_name` body, `set_guest_network(enabled = false)` body): padding is
    // preserved (never trimmed) and the text "None" is just text.
    let cases = [
        (
            "My Network",
            "name=My+Network",
            "enabled=false&name=My+Network",
        ),
        (
            "  preserve spaces  ",
            "name=++preserve+spaces++",
            "enabled=false&name=++preserve+spaces++",
        ),
        ("None", "name=None", "enabled=false&name=None"),
    ];
    for (_, settings_body, guest_body) in cases {
        for (resource, body) in [("settings", settings_body), ("guestnetwork", guest_body)] {
            Mock::given(method("PUT"))
                .and(path(format!("/2.2/networks/network-0001/{resource}")))
                .and(session_cookie())
                .and(user_token_header())
                .and(header("content-type", "application/x-www-form-urlencoded"))
                .and(body_string(body))
                .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
                .expect(1)
                .mount(&mock.server)
                .await;
        }
    }

    let api = networks_api(&mock);
    for (name, _, _) in cases {
        api.set_network_name("network-0001", name, None).await?;
        api.set_guest_network("network-0001", false, Some(name), None)
            .await?;
    }
    Ok(())
}

// ================================ NetworksApi::set_hide_5g ================================

/// Mounts `PUT /2.2/networks/network-0001/temporary_flags/hide_5g` expecting exactly one JSON
/// `{"value": true}` request with both credentials.
async fn mount_hide_5g_put(mock: &MockEero) {
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/temporary_flags/hide_5g"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(json!({ "value": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// Mounts `DELETE /2.2/networks/network-0001/temporary_flags/hide_5g` expecting exactly one
/// body-less request with both credentials, plus a `PUT` mock on the same path that must never fire.
async fn mount_hide_5g_delete(mock: &MockEero) {
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/temporary_flags/hide_5g"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/temporary_flags/hide_5g"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn set_hide_5g_true_puts_value_true_to_the_temporary_flag() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hide_5g_put(&mock).await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/temporary_flags/hide_5g"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let env = networks_api(&mock)
        .set_hide_5g("network-0001", true)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_false_deletes_the_temporary_flag_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hide_5g_delete(&mock).await;

    let env = networks_api(&mock)
        .set_hide_5g("network-0001", false)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert_eq!(requests.len(), 1, "got {requests:?}");
    assert!(requests[0].body.is_empty());
    assert!(
        requests[0].headers.get("content-type").is_none(),
        "a body-less DELETE must not declare a content type: {:?}",
        requests[0].headers
    );
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_accepts_an_api_network_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A `/2.2/networks/{id}` API path resolves to the same single, undoubled path.
    mount_hide_5g_put(&mock).await;

    networks_api(&mock)
        .set_hide_5g("/2.2/networks/network-0001", true)
        .await?;

    let mock = MockEero::start().await;
    mount_hide_5g_delete(&mock).await;
    networks_api(&mock)
        .set_hide_5g("/2.2/networks/network-0001", false)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_accepts_an_absolute_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hide_5g_put(&mock).await;
    let url = format!("{}/2.2/networks/network-0001", mock.uri());

    networks_api(&mock).set_hide_5g(&url, true).await?;

    let mock = MockEero::start().await;
    mount_hide_5g_delete(&mock).await;
    let url = format!("{}/2.2/networks/network-0001", mock.uri());
    networks_api(&mock).set_hide_5g(&url, false).await?;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_warns_once_with_a_different_operation_per_branch() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hide_5g_put(&mock).await;
    let (_guard, logs) = common::capture_logs();
    let api = networks_api(&mock);

    api.set_hide_5g("network-0001", true).await?;
    assert_eq!(logs.warnings().len(), 1, "got {:?}", logs.warnings());
    assert_eq!(
        logs.count(
            tracing::Level::WARN,
            "Issuing write (hide 5 GHz for network)"
        ),
        1
    );
    assert!(!logs.contains(tracing::Level::WARN, "clear hide 5 GHz"));

    let mock = MockEero::start().await;
    mount_hide_5g_delete(&mock).await;
    logs.clear();
    networks_api(&mock)
        .set_hide_5g("network-0001", false)
        .await?;
    assert_eq!(logs.warnings().len(), 1, "got {:?}", logs.warnings());
    assert_eq!(
        logs.count(
            tracing::Level::WARN,
            "Issuing write (clear hide 5 GHz for network)"
        ),
        1
    );
    assert!(!logs.contains(
        tracing::Level::WARN,
        "Issuing write (hide 5 GHz for network)"
    ));
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_with_an_unresolvable_id_sends_nothing_and_does_not_warn() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let (_guard, logs) = common::capture_logs();
    let api = networks_api(&mock);

    for enabled in [true, false] {
        for bad_id in ["", "../x"] {
            let err = api
                .set_hide_5g(bad_id, enabled)
                .await
                .expect_err("an invalid network id must be rejected before any request");
            assert!(
                matches!(err, Error::Validation { .. }),
                "{bad_id:?} / {enabled}: got {err:?}"
            );
        }
    }

    assert!(
        logs.warnings().is_empty(),
        "the warning follows URL resolution: {:?}",
        logs.warnings()
    );
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_not_authenticated_returns_authentication_error_without_a_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    for verb in ["PUT", "DELETE"] {
        Mock::given(method(verb))
            .and(path("/2.2/networks/network-0001/temporary_flags/hide_5g"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(0)
            .mount(&mock.server)
            .await;
    }
    let api = NetworksApi::new(Arc::new(mock.transport_anonymous()));

    for enabled in [true, false] {
        let err = api
            .set_hide_5g("network-0001", enabled)
            .await
            .expect_err("an unauthenticated write must fail");
        assert!(err.is_auth_error(), "enabled={enabled}: got {err:?}");
    }
    assert_no_requests(&mock).await;
    Ok(())
}
