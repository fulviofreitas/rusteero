//! `Client` integration suite for the `power_saving` domain (new in v8.0.0,
//! `eero-api src/eero/client.py:2814-2888`).

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::power_saving::UpdatePowerSavingScheduleOptions;
use rusteero::error::Error;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn set_power_saving_invalidates_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"meta\":{},\"data\":{}}"))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/power_saving"))
        .and(session_cookie())
        .and(body_json(json!({ "enable": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_power_saving(Some("network-0001"), Some(true), None)
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_power_saving_rejects_no_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .set_power_saving(Some("network-0001"), None, None)
        .await
        .expect_err("an empty call must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "power_saving"));
    Ok(())
}

/// `Client::get_power_saving_schedules` passes the cached network as `parent=`
/// (`_network_parent_kwargs`, `client.py:2832-2837`), exactly like every other `+net` wrapper —
/// but `PowerSavingApi::get_schedules`'s route deliberately has `link: None`
/// (`routes::power_saving::POWER_SAVING_GET_SCHEDULES`'s own doc comment), so the parent is
/// structurally inert at the domain layer: even a cached network publishing an (arbitrary,
/// unrelated) `resources` link must never change the resolved path away from the bare-id
/// template.
#[tokio::test]
async fn get_power_saving_schedules_passes_the_cached_network_as_an_inert_parent()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": {"code": 200},
        "data": {
            "url": "/2.2/networks/network-0001",
            "resources": {"schedules": "/2.3/networks/network-0001/power_saving/schedules"},
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/power_saving/schedules"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/power_saving/schedules"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .get_power_saving_schedules(Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_power_saving_schedules_requires_a_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_power_saving_schedules(None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}

#[tokio::test]
async fn create_power_saving_schedule_sends_all_five_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/power_saving/schedules"))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Overnight",
            "days": ["MON"],
            "start_time": "22:00",
            "end_time": "06:00",
            "enabled": true,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .create_power_saving_schedule(
            Some("network-0001"),
            "Overnight",
            json!(["MON"]),
            "22:00",
            "06:00",
            true,
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn update_power_saving_schedule_sends_only_supplied_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/power_saving/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .update_power_saving_schedule(
            "schedule-0001",
            Some("network-0001"),
            &UpdatePowerSavingScheduleOptions {
                enabled: Some(false),
                ..UpdatePowerSavingScheduleOptions::default()
            },
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn delete_power_saving_schedule_sends_a_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/power_saving/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .delete_power_saving_schedule("schedule-0001", Some("network-0001"))
        .await?;
    Ok(())
}
