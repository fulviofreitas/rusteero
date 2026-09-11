//! HTTP integration tests for the read-only half of three small `eero-api` domain modules —
//! `BackupApi`, `BlacklistApi` and `BurstReportersApi` (`src/endpoints/backup.rs`,
//! `src/endpoints/blacklist.rs`, `src/endpoints/burst_reporters.rs`) — against a local
//! `wiremock` server per `.claude/rules/testing.md`.
//!
//! None of these three modules has a committed fixture file, so every response body here is a
//! small, obviously-synthetic `{"meta": …, "data": …}` value built inline with
//! `serde_json::json!`, in the shape `eero-api`'s own tests
//! (`tests/api/test_backup.py`, `test_blacklist.py`, `test_burst_reporters.py`) use — no real
//! MACs, serials, IPs or names. A dedicated
//! `get_backup_network_and_get_backup_status_hit_different_paths` test proves `BackupAPI`'s two
//! `GET`s are distinct wire endpoints, not aliases of one another. A final error-path test
//! proves a `404` maps to `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::backup::BackupApi;
use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::endpoints::burst_reporters::BurstReportersApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

/// Builds a [`BackupApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn backup_api(mock: &MockEero) -> BackupApi {
    BackupApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`BlacklistApi`] pointed at `mock`, wrapping a `Transport` already authenticated
/// with [`TEST_TOKEN`].
fn blacklist_api(mock: &MockEero) -> BlacklistApi {
    BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`BurstReportersApi`] pointed at `mock`, wrapping a `Transport` already
/// authenticated with [`TEST_TOKEN`].
fn burst_reporters_api(mock: &MockEero) -> BurstReportersApi {
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
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

// ===================== get_blacklist =====================

#[tokio::test]
async fn get_blacklist_hits_blacklist_path_and_returns_the_served_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": [
            {"mac": "aa:bb:cc:00:00:01", "device_id": "aabbcc000001"},
            {"mac": "aa:bb:cc:00:00:02", "device_id": "aabbcc000002"},
        ],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = blacklist_api(&mock);
    let env = api.get_blacklist("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_burst_reporters =====================

#[tokio::test]
async fn get_burst_reporters_hits_burst_reporters_path_and_returns_the_served_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": [{"id": "reporter-0001", "type": "burst"}],
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/burst_reporters"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = burst_reporters_api(&mock);
    let env = api.get_burst_reporters("network-0001").await?;
    assert_eq!(env.into_value(), body);
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
