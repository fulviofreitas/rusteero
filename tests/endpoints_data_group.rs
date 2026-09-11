//! P3 suite for the four "awkward" read-only endpoint modules plus `ScheduleApi`:
//! `DataUsageApi`, `InsightsApi`, `DiagnosticsApi`, `OUICheckApi` and `ScheduleApi`, all against
//! a local `wiremock` server per `.claude/rules/testing.md`.
//!
//! Three request-shape regressions get dedicated coverage here because they are the ones most
//! likely to silently regress:
//!
//! - [`DataUsageApi::get_data_usage`] sends a `GET` with a JSON *body* — proven both by
//!   `body_json` matchers and by a same-network mock on the *other* `data_usage`/`data_usage/
//!   {resource}` path asserting `.expect(0)`, so a bug that renders the wrong path would fail
//!   loudly instead of accidentally matching.
//! - [`InsightsApi::get_insights`] sends its four required parameters as *query string* entries,
//!   never a body — proven with `query_param` matchers on all four names.
//! - [`ScheduleApi::get_profile_schedule`] hits the exact same path as `ProfilesApi::get_profile`
//!   and returns the whole profile object untransformed — no `schedule`-field extraction.
//!
//! A final error-path test proves a `404` maps to `Error::Api { status: 404, .. }`.

mod common;

use std::sync::Arc;

use rusteero::endpoints::data_usage::DataUsageApi;
use rusteero::endpoints::diagnostics::DiagnosticsApi;
use rusteero::endpoints::insights::InsightsApi;
use rusteero::endpoints::ouicheck::OUICheckApi;
use rusteero::endpoints::schedule::ScheduleApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

fn data_usage_api(mock: &MockEero) -> DataUsageApi {
    DataUsageApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn insights_api(mock: &MockEero) -> InsightsApi {
    InsightsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn diagnostics_api(mock: &MockEero) -> DiagnosticsApi {
    DiagnosticsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn ouicheck_api(mock: &MockEero) -> OUICheckApi {
    OUICheckApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn schedule_api(mock: &MockEero) -> ScheduleApi {
    ScheduleApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_data_usage =====================

#[tokio::test]
async fn get_data_usage_without_resource_sends_the_body_to_the_bare_data_usage_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "timezone": "America/New_York", "period": "day" });
    let response = json!({
        "meta": { "code": 200 },
        "data": { "download": 1_024_000, "upload": 512_000 },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // If `resource: None` ever accidentally rendered the resource-scoped route, this mock —
    // registered on the sibling path — would be the one to receive the request instead.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api.get_data_usage("network-0001", payload, None).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_data_usage_with_resource_sends_the_body_to_the_resource_scoped_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({ "timezone": "America/New_York" });
    let response = json!({
        "meta": { "code": 200 },
        "data": [{ "device_id": "dev_1", "download": 100 }],
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .and(session_cookie())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // If `resource: Some("devices")` ever accidentally rendered the bare route, this mock —
    // registered on the sibling path — would be the one to receive the request instead.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let env = api
        .get_data_usage("network-0001", payload, Some("devices"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
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

// ===================== get_diagnostics =====================

#[tokio::test]
async fn get_diagnostics_hits_networks_id_diagnostics_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "network_health": "good", "internet_status": "connected" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/diagnostics"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let env = api.get_diagnostics("network-0001").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== get_ouicheck =====================

#[tokio::test]
async fn get_ouicheck_hits_networks_id_ouicheck_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "vendor": "Apple", "mac": "AA:BB:CC:DD:EE:FF" },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ouicheck_api(&mock);
    let env = api.get_ouicheck("network-0001").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== get_profile_schedule =====================

#[tokio::test]
async fn get_profile_schedule_hits_networks_profiles_id_and_returns_the_whole_profile()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": {
            "name": "Kids",
            "schedule": [{ "days": ["monday"], "start": "21:00", "end": "07:00" }],
        },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let env = api
        .get_profile_schedule("network-0001", "profile-0001")
        .await?;
    // Untransformed: the whole profile object comes back, not just its `schedule` field.
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_diagnostics_with_unknown_network_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/diagnostics"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = diagnostics_api(&mock);
    let err = api
        .get_diagnostics("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
