//! HTTP integration tests for `ReservationsApi` (`src/endpoints/reservations.rs`) against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! `ReservationsApi` has no committed fixture under `tests/fixtures/`, so every test here builds
//! a small inline `{"meta": {...}, "data": {...}}` body with `serde_json::json!`, shaped like the
//! payloads `eero-api`'s own tests use (`tests/api/test_reservations.py`) but with obviously
//! synthetic MACs and IPs drawn from the `192.0.2.0/24`/`192.168.4.0/24` documentation ranges
//! rather than anything real.

mod common;

use std::sync::Arc;

use rusteero::endpoints::reservations::ReservationsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn reservations_api(mock: &MockEero) -> ReservationsApi {
    ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_reservations =====================

#[tokio::test]
async fn get_reservations_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": [{ "ip": "192.168.4.100", "mac": "aa:bb:cc:00:00:01" }],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.get_reservations("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_reservations_prefers_the_parents_published_link_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"reservations": "/2.3/networks/network-0001/reservations"}});
    let body = json!({ "meta": { "code": 200 }, "data": [] });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/reservations"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.get_reservations("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_reservations_falls_back_to_template_when_parent_has_no_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {}});
    let body = json!({ "meta": { "code": 200 }, "data": [] });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.get_reservations("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== create_reservation =====================

#[tokio::test]
async fn create_reservation_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "mac": "aa:bb:cc:00:00:09", "ip": "192.168.4.109" });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "url": "/2.2/networks/network-0001/reservations/res-0001" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .create_reservation("network-0001", payload, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== update_reservation =====================

#[tokio::test]
async fn update_reservation_from_path_string_needs_no_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "public_static_ip": true });
    let response = json!({ "meta": { "code": 200 }, "data": { "public_static_ip": true } });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .update_reservation(
            "/2.2/networks/network-0001/reservations/res-0001",
            payload,
            None,
            None,
        )
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_reservation_from_envelope_uses_its_own_url_even_on_a_different_version()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"url": "/2.3/networks/network-0001/reservations/res-0001"});
    let payload = json!({ "public_static_ip": true });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .update_reservation("res-0001", payload, None, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_reservation_bare_id_requires_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = reservations_api(&mock);

    let err = api
        .update_reservation("res-0001", json!({}), None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "network"));
    Ok(())
}

#[tokio::test]
async fn update_reservation_bare_id_with_network_resolves_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .update_reservation("res-0001", json!({}), Some("network-0001"), None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_reservation_envelope_with_no_url_is_a_validation_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"id": "res-0001"});
    let api = reservations_api(&mock);

    let err = api
        .update_reservation("res-0001", json!({}), None, Some(&parent))
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "reservation"));
    Ok(())
}

// ===================== delete_reservation =====================

#[tokio::test]
async fn delete_reservation_with_no_delete_forwards_omits_the_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .delete_reservation("network-0001", "res-0001", None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn delete_reservation_delete_forwards_true_sends_string_true() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(query_param("delete_forwards", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .delete_reservation("network-0001", "res-0001", Some(true))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn delete_reservation_delete_forwards_false_sends_string_false() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(query_param("delete_forwards", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api
        .delete_reservation("network-0001", "res-0001", Some(false))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
