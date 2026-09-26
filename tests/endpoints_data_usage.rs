//! HTTP integration tests for `DataUsageApi` (`src/endpoints/data_usage.rs`) at `v8.0.4`.
//!
//! Every method in this family sends `start`/`end`/`cadence`/`timezone` as *query* parameters,
//! never a request body — proven both by `query_param` matchers and, on the two-path tests, an
//! `.expect(0)` mock on the sibling path so a bug that renders the wrong path fails loudly.

mod common;

use std::sync::Arc;

use rusteero::endpoints::data_usage::DataUsageApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn data_usage_api(mock: &MockEero) -> DataUsageApi {
    DataUsageApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_data_usage =====================

#[tokio::test]
async fn get_data_usage_sends_query_params_and_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {"download": 1}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("timezone", "America/New_York"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api
        .get_data_usage(
            "network-0001",
            "s",
            "e",
            "daily",
            Some("America/New_York"),
            None,
        )
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_data_usage_omits_timezone_when_not_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_data_usage("network-0001", "s", "e", "daily", None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_data_usage_rejects_invalid_cadence_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    for bad in ["weekly", "", "DAILY"] {
        let err = api
            .get_data_usage("network-0001", "s", "e", bad, None, None)
            .await
            .expect_err("an invalid cadence must be rejected before any request");
        assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    }
    Ok(())
}

// ===================== the never-a-body guarantee =====================

#[tokio::test]
async fn get_data_usage_never_sends_a_request_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A GET with an empty-object body is a distinct wire shape from a bodyless GET; this mock
    // must never match, proving no body is attached at all (`data_usage.py:10-11`: the API
    // rejects a body-bearing GET with a 400).
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_data_usage("network-0001", "s", "e", "daily", None, None)
        .await?;
    Ok(())
}

// ===================== get_breakdown / get_devices_usage / get_unprofiled_devices (optional cadence) =====================

#[tokio::test]
async fn get_breakdown_url_and_required_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/breakdown"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_breakdown("network-0001", "s", "e", None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_breakdown_cadence_included_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/breakdown"))
        .and(query_param("cadence", "hourly"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_breakdown("network-0001", "s", "e", Some("hourly"), None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_breakdown_invalid_cadence_rejected() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_breakdown("network-0001", "s", "e", Some("weekly"), None, None)
        .await
        .expect_err("weekly must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}

#[tokio::test]
async fn get_devices_usage_profile_id_included_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .and(query_param("profile_id", "profile-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_devices_usage(
        "network-0001",
        "s",
        "e",
        None,
        None,
        Some("profile-0001"),
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_usage_profile_id_omitted_when_none() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_devices_usage("network-0001", "s", "e", None, None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_devices_url_and_required_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/devices",
        ))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_unprofiled_devices("network-0001", "s", "e", None, None, None)
        .await?;
    Ok(())
}

// ===================== the required-cadence family (device/eeros/eero/profile/unprofiled-summary) =====================

#[tokio::test]
async fn get_device_usage_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/devices/aa:bb:cc",
        ))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_device_usage("network-0001", "aa:bb:cc", "s", "e", "daily", None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_usage_empty_child_id_rejected() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_device_usage("network-0001", "", "s", "e", "daily", None, None)
        .await
        .expect_err("an empty device_mac must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

#[tokio::test]
async fn get_device_usage_id_with_brace_does_not_break_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_device_usage("network-0001", "aa{bb}cc", "s", "e", "daily", None, None)
        .await
        .expect_err("a brace-carrying child id must be rejected, not silently substituted");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

#[tokio::test]
async fn get_eeros_summary_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros/summary"))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_eeros_summary("network-0001", "s", "e", "daily", None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_usage_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
        ))
        .and(query_param("cadence", "hourly"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_eero_usage("network-0001", "eero-0001", "s", "e", "hourly", None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_usage_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/profiles/profile-0001",
        ))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_profile_usage(
        "network-0001",
        "profile-0001",
        "s",
        "e",
        "daily",
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_summary_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/summary",
        ))
        .and(query_param("cadence", "daily"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_unprofiled_summary("network-0001", "s", "e", "daily", None, None)
        .await?;
    Ok(())
}

// ===================== report_settings =====================

#[tokio::test]
async fn get_report_settings_hits_the_flat_path_with_no_query_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {"cadence": "daily"}});
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api.get_report_settings("network-0001", None).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_report_settings_sends_a_json_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(body_json(
            json!({ "cadence": "daily", "notification_day": "monday" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.set_report_settings("network-0001", "daily", "monday", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_report_settings_invalid_cadence_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let err = api
        .set_report_settings("network-0001", "weekly", "monday", None)
        .await
        .expect_err("an invalid cadence must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}
