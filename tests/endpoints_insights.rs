//! HTTP integration tests for `InsightsApi` (`src/endpoints/insights.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! [`InsightsApi::get_insights`] sends its four required parameters as *query string* entries,
//! never a body — proven with `query_param` matchers on all four names.
//! `InsightsApi::run_insights` posts exactly an empty object body.

mod common;

use std::sync::Arc;

use rusteero::endpoints::insights::InsightsApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

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

// ===================== run_insights =====================

#[tokio::test]
async fn run_insights_posts_exactly_an_empty_object_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "status": "running" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/insights"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = insights_api(&mock);
    let env = api.run_insights("network-0001").await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
