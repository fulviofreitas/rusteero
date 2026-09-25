//! HTTP integration tests for `BackupApi` (`src/endpoints/backup.rs`) against a local `wiremock`
//! server per the crate's testing conventions.
//!
//! None of `BackupApi`'s methods has a committed fixture file, so every response body here is a
//! small, obviously-synthetic `{"meta": …, "data": …}` value built inline with
//! `serde_json::json!`, in the shape `eero-api`'s own tests (`tests/api/test_backup.py`) use —
//! no real MACs, serials, IPs or names. A dedicated
//! `get_backup_network_and_get_backup_status_hit_different_paths` test proves `BackupAPI`'s two
//! `GET`s are distinct wire endpoints, not aliases of one another. A final error-path test
//! proves a `404` maps to `Error::Api { status: 404, .. }`.
//!
//! The mutating half (`set_backup_network`, `configure_backup_network`) pins the exact verb,
//! path, JSON body and session cookie via `body_json`, with `.expect(n)` call counts verified at
//! server-drop time.
//! `configure_backup_network_with_no_arguments_is_validation_error_with_zero_requests` was
//! red/green-verified by hand (expected body deliberately broken, observed panic, restored,
//! observed pass); see this crate's task report for the captured output.

mod common;

use std::sync::Arc;

use rusteero::endpoints::backup::BackupApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

/// Builds a [`BackupApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn backup_api(mock: &MockEero) -> BackupApi {
    BackupApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_backup_network =====================

#[tokio::test]
async fn get_backup_network_hits_backup_path_and_returns_the_served_envelope() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"enabled": true, "status": "standby", "phone_number": "+15555550100"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.get_backup_network("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_backup_status =====================

#[tokio::test]
async fn get_backup_status_hits_backup_status_path_and_returns_the_served_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"active": true, "connected": true, "signal_strength": 75},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup/status"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.get_backup_status("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_backup_network vs get_backup_status: distinct paths =====================

#[tokio::test]
async fn get_backup_network_and_get_backup_status_hit_different_paths() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Two separate mocks, each pinned to its own exact path with `expect(1)`: if either method
    // rendered the other's path (or if both collapsed onto one shared route), one mock would
    // receive zero calls and wiremock's own call-count verification (run when `mock.server` is
    // dropped) would fail the test — independently of the body assertions below.
    let network_body = json!({"meta": {"code": 200}, "data": {"enabled": false}});
    let status_body = json!({"meta": {"code": 200}, "data": {"active": false}});

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup/status"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(status_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let network_env = api.get_backup_network("network-0001").await?;
    let status_env = api.get_backup_status("network-0001").await?;

    assert_eq!(network_env.into_value(), network_body);
    assert_eq!(status_env.into_value(), status_body);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_backup_network_with_unknown_network_id_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/backup"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let err = api
        .get_backup_network("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== set_backup_network =====================

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

// ===================== configure_backup_network =====================

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
