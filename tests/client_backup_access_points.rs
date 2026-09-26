//! `Client` integration suite for `BackupAccessPointsApi` (new in v8.0.0): network-id resolution
//! and `list`'s `+net` parent passthrough.

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::backup_access_points::UpdateBackupAccessPointOptions;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn list_backup_access_points_passes_the_cached_network_envelope_as_parent()
-> anyhow::Result<()> {
    // The cached network envelope below carries `resources.backup_access_points` pointing at a
    // `/2.3` path — a fresh cache read after `get_network` must make
    // `list_backup_access_points` prefer that link over the `/2.2` template.
    let mock = MockEero::start().await;
    let network_body = json!({
        "meta": {"code": 200},
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "backup_access_points": "/2.3/networks/network-0001/backup_access_points",
            },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let list_body = json!({ "meta": { "code": 200 }, "data": { "backup_access_points": [] } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(list_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let env = client
        .list_backup_access_points(Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), list_body);
    Ok(())
}

#[tokio::test]
async fn list_backup_access_points_falls_back_to_the_template_with_an_empty_cache()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let list_body = json!({ "meta": { "code": 200 }, "data": { "backup_access_points": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(list_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .list_backup_access_points(Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), list_body);
    Ok(())
}

#[tokio::test]
async fn add_backup_access_point_resolves_network_id_and_sends_the_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/backup_access_points"))
        .and(session_cookie())
        .and(body_json(json!({ "ssid": "guest", "password": "pw" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .add_backup_access_point("guest", "pw", None, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn update_backup_access_point_forwards_the_supplied_options() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/backup_access_points/backup-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let options = UpdateBackupAccessPointOptions {
        enabled: Some(false),
        ..UpdateBackupAccessPointOptions::default()
    };
    let env = client
        .update_backup_access_point("backup-0001", &options, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}
