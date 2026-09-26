//! HTTP integration tests for `InsightsApi` (`src/endpoints/insights.rs`) at `v8.0.4`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::insights::InsightsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn insights_api(mock: &MockEero) -> InsightsApi {
    InsightsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_insights =====================

#[tokio::test]
async fn get_insights_sends_all_four_parameters_in_the_query_string() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "series": [{ "insight_type": "adblock" }] },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights"))
        .and(session_cookie())
        .and(user_token_header())
        .and(query_param("start", "2026-07-21T00:00:00Z"))
        .and(query_param("end", "2026-07-22T00:00:00Z"))
        .and(query_param("insight_type", "adblock"))
        .and(query_param("cadence", "hourly"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let env = api
        .get_insights(
            "network-0001",
            "2026-07-21T00:00:00Z",
            "2026-07-22T00:00:00Z",
            "adblock",
            "hourly",
        )
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_insights_rejects_cadence_outside_the_api_set_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let err = api
        .get_insights(
            "network-0001",
            "2026-07-21T00:00:00Z",
            "2026-07-22T00:00:00Z",
            "adblock",
            "weekly",
        )
        .await
        .expect_err("\"weekly\" is no longer an accepted cadence at v8.0.4");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}

// ===================== get_devices_insights =====================

#[tokio::test]
async fn get_devices_insights_builds_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {"series": []}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights/devices"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("insight_type", "adblock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let env = api
        .get_devices_insights("network-0001", "s", "e", "daily", "adblock", None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_devices_insights_prefers_parents_insights_devices_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/insights/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent =
        json!({"resources": {"insights_devices": "/2.4/networks/network-0001/insights/devices"}});
    let api = insights_api(&mock);
    api.get_devices_insights("network-0001", "s", "e", "daily", "adblock", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_insights_invalid_cadence_raises() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = insights_api(&mock);
    let err = api
        .get_devices_insights("network-0001", "s", "e", "weekly", "adblock", None)
        .await
        .expect_err("weekly must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}

// ===================== get_device_insights =====================

#[tokio::test]
async fn get_device_insights_builds_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": {"series": []}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights/devices/aa:bb:cc"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "hourly"))
        .and(query_param("insight_type", "adblock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let env = api
        .get_device_insights("network-0001", "aa:bb:cc", "s", "e", "hourly", "adblock")
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_device_insights_network_id_with_brace_does_not_break_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = insights_api(&mock);
    // `{` is outside `validate_identifier`'s allowed character class, so this must be rejected
    // as a validation error, never reach a second `.format()`-style substitution.
    let err = api
        .get_device_insights("network{0}", "aa:bb:cc", "s", "e", "hourly", "adblock")
        .await
        .expect_err("a brace-carrying network id must be rejected, not silently substituted");
    assert!(matches!(err, Error::Validation { .. }));
    Ok(())
}

// ===================== get_profiles_insights / get_profile_insights / get_profile_devices_insights =====================

#[tokio::test]
async fn get_profiles_insights_builds_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/insights/profiles"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("insight_type", "adblock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    api.get_profiles_insights("network-0001", "s", "e", "daily", "adblock", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_insights_builds_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/insights/profiles/profile-0001",
        ))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("insight_type", "adblock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    api.get_profile_insights("network-0001", "profile-0001", "s", "e", "daily", "adblock")
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_devices_insights_builds_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/insights/profiles/profile-0001/devices",
        ))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("insight_type", "adblock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    api.get_profile_devices_insights("network-0001", "profile-0001", "s", "e", "daily", "adblock")
        .await?;
    Ok(())
}
