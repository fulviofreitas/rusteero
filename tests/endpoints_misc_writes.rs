//! HTTP integration tests for the mutating (`PUT`/`POST`/`DELETE`) halves of eight small
//! `eero-api` domain modules: `BackupApi::{set_backup_network, configure_backup_network}`
//! (`src/endpoints/backup.rs`), `ReservationsApi::{create_reservation, update_reservation,
//! delete_reservation}` (`src/endpoints/reservations.rs`),
//! `ForwardsApi::{create_forward, delete_forward}` (`src/endpoints/forwards.rs`),
//! `DiagnosticsApi::run_diagnostics` (`src/endpoints/diagnostics.rs`),
//! `InsightsApi::run_insights` (`src/endpoints/insights.rs`), `OUICheckApi::run_ouicheck`
//! (`src/endpoints/ouicheck.rs`), `SupportApi::request_support` (`src/endpoints/support.rs`) and
//! `BurstReportersApi::create_burst_reporter` (`src/endpoints/burst_reporters.rs`).
//!
//! Per `.claude/rules/testing.md`, every test pins the exact verb, path, JSON body (via
//! `body_json`) and session cookie against a local `wiremock` server, with `.expect(n)` call
//! counts verified when the mock server is dropped. Bodies use obviously-synthetic MACs
//! (`aa:bb:cc:00:00:0N`) and IPs drawn from the `192.0.2.0/24` documentation range (RFC 5737) —
//! never anything real. `create_reservation` and `request_support` each carry a nested object to
//! prove a passthrough body arrives byte-identical, nesting included.
//!
//! `configure_backup_network_with_no_arguments_is_validation_error_with_zero_requests` and
//! `create_reservation_passthrough_body_arrives_byte_identical_including_nested_object` were
//! each red-green verified by hand: a temporary edit to the method under test (respectively,
//! deleting the empty-payload guard, and swapping the passed-through body for an empty object)
//! was made, the test was confirmed to fail for exactly the expected reason, and the edit was
//! reverted before this file was considered done. See this crate's task report for the observed
//! RED output of both runs.

mod common;

use std::sync::Arc;

use rusteero::endpoints::backup::BackupApi;
use rusteero::endpoints::burst_reporters::BurstReportersApi;
use rusteero::endpoints::diagnostics::DiagnosticsApi;
use rusteero::endpoints::forwards::ForwardsApi;
use rusteero::endpoints::insights::InsightsApi;
use rusteero::endpoints::ouicheck::OUICheckApi;
use rusteero::endpoints::reservations::ReservationsApi;
use rusteero::endpoints::support::SupportApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn backup_api(mock: &MockEero) -> BackupApi {
    BackupApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn reservations_api(mock: &MockEero) -> ReservationsApi {
    ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn forwards_api(mock: &MockEero) -> ForwardsApi {
    ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn diagnostics_api(mock: &MockEero) -> DiagnosticsApi {
    DiagnosticsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn insights_api(mock: &MockEero) -> InsightsApi {
    InsightsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn ouicheck_api(mock: &MockEero) -> OUICheckApi {
    OUICheckApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn support_api(mock: &MockEero) -> SupportApi {
    SupportApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn burst_reporters_api(mock: &MockEero) -> BurstReportersApi {
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== BackupApi::set_backup_network =====================

#[tokio::test]
async fn set_backup_network_puts_enabled_body_to_backup_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "enabled": true } });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.set_backup_network("network-0001", true).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== BackupApi::configure_backup_network =====================

#[tokio::test]
async fn configure_backup_network_with_no_arguments_is_validation_error_with_zero_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A mock IS registered so a stray request would get an answer instead of hanging, but
    // `.expect(0)` means the mock server's drop-time verification fails the test the moment this
    // mock is ever hit — if `configure_backup_network` ever built and sent a request for an
    // empty call, that is what would fail this test, not the `Error::Validation` assertion below.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backup"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let err = api
        .configure_backup_network("network-0001", None, None)
        .await
        .expect_err("a call with neither `enabled` nor `phone_number` must be rejected");

    assert!(matches!(err, Error::Validation { .. }));
    Ok(())
}

#[tokio::test]
async fn configure_backup_network_with_only_enabled_sends_just_that_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "enabled": false } });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        // No `phone_number` key at all — not even `null` — proves the omitted argument's key is
        // absent from the body entirely, rather than sent as `{"enabled": false,
        // "phone_number": null}`.
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api
        .configure_backup_network("network-0001", Some(false), None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn configure_backup_network_with_only_phone_number_sends_just_that_key() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "phone_number": "+15555550100" } });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        // No `enabled` key at all — proves the two optional fields are populated independently.
        .and(body_json(json!({ "phone_number": "+15555550100" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api
        .configure_backup_network("network-0001", None, Some("+15555550100"))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== ReservationsApi::create_reservation =====================

#[tokio::test]
async fn create_reservation_passthrough_body_arrives_byte_identical_including_nested_object()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "mac": "aa:bb:cc:00:00:01",
        "ip": "192.0.2.10",
        "name": "test-reservation",
        "schedule": {
            "enabled": true,
            "days": ["mon", "tue"],
            "window": { "start": "22:00", "end": "06:00" },
        },
    });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "url": "/2.2/networks/network-0001/reservations/res-0001" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.create_reservation("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== ReservationsApi::update_reservation =====================

#[tokio::test]
async fn update_reservation_puts_passthrough_body_to_reservation_id_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "name": "renamed-reservation", "ip": "192.0.2.11" });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

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
        .update_reservation("network-0001", "res-0001", payload)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== ReservationsApi::delete_reservation =====================

#[tokio::test]
async fn delete_reservation_hits_reservation_id_path_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = reservations_api(&mock);
    let env = api.delete_reservation("network-0001", "res-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== ForwardsApi::create_forward =====================

#[tokio::test]
async fn create_forward_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "mac": "aa:bb:cc:00:00:02",
        "internal_ip": "192.0.2.20",
        "internal_port": 8080,
        "external_port": 80,
        "protocol": "tcp",
    });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "url": "/2.2/networks/network-0001/forwards/fwd-0001" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/forwards"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.create_forward("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== ForwardsApi::delete_forward =====================

#[tokio::test]
async fn delete_forward_hits_forward_id_path_with_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/forwards/fwd-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = forwards_api(&mock);
    let env = api.delete_forward("network-0001", "fwd-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== DiagnosticsApi::run_diagnostics =====================

#[tokio::test]
async fn run_diagnostics_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api.run_diagnostics("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== InsightsApi::run_insights =====================

#[tokio::test]
async fn run_insights_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/insights"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let env = api.run_insights("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== OUICheckApi::run_ouicheck =====================

#[tokio::test]
async fn run_ouicheck_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api.run_ouicheck("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== SupportApi::request_support =====================

#[tokio::test]
async fn request_support_passthrough_body_arrives_byte_identical_including_nested_object()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "issue": "wifi-dropping",
        "contact": { "email": "user@example.com", "phone": "+15555550100" },
    });
    let response = json!({ "meta": { "code": 200 }, "data": { "ticket_id": "tick-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = support_api(&mock);
    let env = api.request_support("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== BurstReportersApi::create_burst_reporter =====================

#[tokio::test]
async fn create_burst_reporter_passthrough_body_arrives_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "type": "burst", "target": "192.0.2.30" });
    let response = json!({ "meta": { "code": 200 }, "data": { "id": "reporter-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api.create_burst_reporter("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
