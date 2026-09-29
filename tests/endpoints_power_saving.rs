//! HTTP integration tests for `PowerSavingApi` (`src/endpoints/power_saving.rs`), pinned against
//! `eero-api src/eero/api/power_saving.py` at v8.0.4.

mod common;

use std::sync::Arc;

use rusteero::endpoints::power_saving::{PowerSavingApi, UpdatePowerSavingScheduleOptions};
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn power_saving_api(mock: &MockEero) -> PowerSavingApi {
    PowerSavingApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== set_power_saving =====================

#[tokio::test]
async fn set_power_saving_sends_only_the_supplied_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/power_saving"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "enable": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    api.set_power_saving("network-0001", Some(true), None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_power_saving_sends_both_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/power_saving"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({ "enable": true, "power_saving_schedule_enabled": false }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    api.set_power_saving("network-0001", Some(true), Some(false), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_power_saving_prefers_a_parent_supplied_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/power_saving"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"power_saving": "/2.3/networks/network-0001/power_saving"}});
    let api = power_saving_api(&mock);
    api.set_power_saving("network-0001", Some(true), None, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_power_saving_rejects_no_fields_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = power_saving_api(&mock);
    let err = api
        .set_power_saving("network-0001", None, None, None)
        .await
        .expect_err("an empty call must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "power_saving"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== get_schedules =====================

#[tokio::test]
async fn get_schedules_returns_the_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {"schedules": []}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/power_saving/schedules"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    let env = api.get_schedules("network-0001", None).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// Deliberate asymmetry (verified v8.0.4 behaviour, not a bug): `get_schedules` accepts `parent`
// but never consults it, unlike every other method in this module — a parent carrying a
// `power_saving` link (or any `resources` link at all) must NOT redirect this request.
#[tokio::test]
async fn get_schedules_ignores_a_parent_supplied_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/power_saving/schedules"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"meta\":{},\"data\":{}}"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/power_saving/schedules"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({
        "resources": {"power_saving": "/2.3/networks/network-0001/power_saving/schedules"},
    });
    let api = power_saving_api(&mock);
    api.get_schedules("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== create_schedule =====================

#[tokio::test]
async fn create_schedule_sends_all_five_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let days = json!(["MON", "TUE", "WED"]);
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/power_saving/schedules"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({
            "name": "Overnight",
            "days": days,
            "start_time": "22:00",
            "end_time": "06:00",
            "enabled": true,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    api.create_schedule(
        "network-0001",
        "Overnight",
        json!(["MON", "TUE", "WED"]),
        "22:00",
        "06:00",
        true,
    )
    .await?;
    Ok(())
}

// ===================== update_schedule =====================

#[tokio::test]
async fn update_schedule_sends_only_the_supplied_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/power_saving/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    api.update_schedule(
        "network-0001",
        "schedule-0001",
        &UpdatePowerSavingScheduleOptions {
            enabled: Some(false),
            ..UpdatePowerSavingScheduleOptions::default()
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_schedule_rejects_no_fields_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = power_saving_api(&mock);
    let err = api
        .update_schedule(
            "network-0001",
            "schedule-0001",
            &UpdatePowerSavingScheduleOptions::default(),
        )
        .await
        .expect_err("an empty call must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "schedule"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== delete_schedule =====================

#[tokio::test]
async fn delete_schedule_sends_a_delete_to_the_nested_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/power_saving/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = power_saving_api(&mock);
    api.delete_schedule("network-0001", "schedule-0001").await?;
    Ok(())
}
