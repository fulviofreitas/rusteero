//! HTTP integration tests for `BackupApi` (`src/endpoints/backup.rs`, rewritten for v8.0.4)
//! against a local `wiremock` server per the crate's testing conventions.
//!
//! None of `BackupApi`'s methods has a committed fixture file, so every response body here is a
//! small, obviously-synthetic `{"meta": …, "data": …}` value built inline with
//! `serde_json::json!`, in the shape `eero-api`'s own tests (`tests/api/test_backup.py`) use —
//! no real MACs, serials, IPs or names.

mod common;

use std::sync::Arc;

use rusteero::endpoints::backup::BackupApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds a [`BackupApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn backup_api(mock: &MockEero) -> BackupApi {
    BackupApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_backup_internet =====================

#[tokio::test]
async fn get_backup_internet_hits_backupinternet_path_and_returns_the_served_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "enabled": false } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backupinternet"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.get_backup_internet("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_backup_internet_ignores_a_supplied_parent_envelope() -> anyhow::Result<()> {
    // `get_backup_internet`'s resource is built directly via `resource_url`, never a published
    // link (`routes::GET_BACKUP_INTERNET::link` is `None`) — a parent carrying a competing
    // `resources.backupinternet` link must be ignored, and the bare-id template path must still
    // be hit.
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "enabled": true } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backupinternet"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent =
        json!({"resources": {"backupinternet": "/2.5/networks/network-0001/backupinternet"}});
    let api = backup_api(&mock);
    let env = api
        .get_backup_internet("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== set_backup_internet =====================

#[tokio::test]
async fn set_backup_internet_puts_backup_internet_enabled_body_to_backupinternet_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "enabled": true } });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backupinternet"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "backup_internet_enabled": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.set_backup_internet("network-0001", true, None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== get_cellular_backup_usage =====================

#[tokio::test]
async fn get_cellular_backup_usage_hits_its_own_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "bytes_used": 1024 } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/cellular_backup_usage"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.get_cellular_backup_usage("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_cellular_backup_events =====================

#[tokio::test]
async fn get_cellular_backup_events_hits_its_own_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/cellular_backup_events"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let env = api.get_cellular_backup_events("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_backup_internet_with_unknown_network_id_maps_404_to_not_found() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/backupinternet"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = backup_api(&mock);
    let err = api
        .get_backup_internet("does-not-exist", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
