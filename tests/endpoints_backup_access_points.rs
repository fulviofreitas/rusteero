//! HTTP integration tests for `BackupAccessPointsApi` (`src/endpoints/backup_access_points.rs`,
//! new in v8.0.0) against a local `wiremock` server per the crate's testing conventions.
//!
//! No committed fixture file exists for this domain, so every response body here is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_backup_access_points.py`) use — no real MACs, serials, IPs or
//! names.

mod common;

use std::sync::Arc;

use rusteero::endpoints::backup_access_points::{
    BackupAccessPointsApi, UpdateBackupAccessPointOptions,
};
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds a [`BackupAccessPointsApi`] pointed at `mock`, wrapping a `Transport` already
/// authenticated with [`TEST_TOKEN`].
fn api(mock: &MockEero) -> BackupAccessPointsApi {
    BackupAccessPointsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== list =====================

#[tokio::test]
async fn list_hits_the_collection_path_and_returns_the_served_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "backup_access_points": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).list("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn list_prefers_a_parent_supplied_link_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "backup_access_points": [] } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({
        "resources": {"backup_access_points": "/2.3/networks/network-0001/backup_access_points"},
    });
    let env = api(&mock).list("network-0001", Some(&parent)).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn list_falls_back_to_the_bare_id_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "backup_access_points": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).list("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== add =====================

#[tokio::test]
async fn add_sends_ssid_and_password_and_omits_uuid_when_not_given() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .and(body_json(
            json!({ "ssid": "guest-ap", "password": "s3cr3t-pass" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .add("network-0001", "guest-ap", "s3cr3t-pass", None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn add_includes_uuid_when_given() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .and(body_json(
            json!({ "ssid": "guest-ap", "password": "s3cr3t-pass", "uuid": "uuid-0001" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .add("network-0001", "guest-ap", "s3cr3t-pass", Some("uuid-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== update =====================

#[tokio::test]
async fn update_sends_only_the_supplied_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/backup-0001",
        ))
        .and(session_cookie())
        // No `password`/`uuid`/`connectivity`/`created`/`last_updated_at` keys at all — proves
        // the omitted fields' keys are absent from the body entirely, never sent as `null`.
        .and(body_json(json!({ "ssid": "new-ssid", "enabled": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let options = UpdateBackupAccessPointOptions {
        ssid: Some("new-ssid"),
        enabled: Some(true),
        ..UpdateBackupAccessPointOptions::default()
    };
    let env = api(&mock)
        .update("network-0001", "backup-0001", &options)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== delete_backup_access_point =====================

#[tokio::test]
async fn delete_backup_access_point_sends_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/backup-0001",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock)
        .delete_backup_access_point("network-0001", "backup-0001")
        .await?;
    Ok(())
}

// ===================== rearrange =====================

#[tokio::test]
async fn rearrange_sends_rearranged_ids_in_order() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/rearrange",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "rearranged_ids": ["b1", "b2", "b3"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .rearrange("network-0001", &["b1", "b2", "b3"])
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== discover_ssids =====================

#[tokio::test]
async fn discover_ssids_hits_ssid_discovery_path_with_get() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "discovered_ssids": [] } });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/ssid_discovery",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).discover_ssids("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== start_ssid_discovery =====================

#[tokio::test]
async fn start_ssid_discovery_sends_the_literal_empty_json_string_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/ssid_discovery",
        ))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock).start_ssid_discovery("network-0001").await?;
    Ok(())
}

// ===================== connectivity_check =====================

#[tokio::test]
async fn connectivity_check_sends_the_literal_empty_json_string_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/connectivity_check",
        ))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock).connectivity_check("network-0001").await?;
    Ok(())
}
