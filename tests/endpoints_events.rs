//! HTTP integration tests for `EventsApi` (`src/endpoints/events.rs`, new in v8.0.0) against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! No committed fixture file exists for this domain, so every response body here is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_events.py`) use — no real MACs, serials, IPs or names.
//!
//! Every method resolves its URL from the network's own self-url-preferring resolution rather
//! than a named `resources` link (see `src/endpoints/events.rs`'s module docs) — the
//! "prefers-parent" tests below assert that by supplying a parent whose `url` field names a
//! *different* API-version prefix (`/2.3`) than the default (`/2.2`) and mounting only the
//! `/2.3` mock.

mod common;

use std::sync::Arc;

use rusteero::endpoints::events::{EventsApi, GetChannelUtilizationOptions};
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds an [`EventsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn api(mock: &MockEero) -> EventsApi {
    EventsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_app_events =====================

#[tokio::test]
async fn get_app_events_with_no_optional_params_sends_no_query_string() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/app_events"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .get_app_events("network-0001", None, None, None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_app_events_sends_the_supplied_query_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/app_events"))
        .and(session_cookie())
        .and(query_param("page_size", "10"))
        .and(query_param("timestamp", "2026-01-01T00:00:00Z"))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .get_app_events("network-0001", Some(10), Some("2026-01-01T00:00:00Z"), None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_app_events_prefers_a_parent_supplied_self_url_over_the_template() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/app_events"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let env = api(&mock)
        .get_app_events("network-0001", None, None, Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_app_events_falls_back_to_the_bare_id_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "events": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/app_events"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .get_app_events("network-0001", None, None, None)
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_network_scan =====================

#[tokio::test]
async fn get_network_scan_hits_the_network_scan_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "scan": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/network_scan"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).get_network_scan("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_network_scan_prefers_a_parent_supplied_self_url_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "scan": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/network_scan"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/network_scan"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let env = api(&mock)
        .get_network_scan("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_channel_utilization =====================

#[tokio::test]
async fn get_channel_utilization_with_required_params_only() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .and(session_cookie())
        .and(query_param("start", "2026-01-01T00:00:00Z"))
        .and(query_param("end", "2026-01-02T00:00:00Z"))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions::default();
    let env = api(&mock)
        .get_channel_utilization(
            "network-0001",
            "2026-01-01T00:00:00Z",
            "2026-01-02T00:00:00Z",
            &options,
            None,
        )
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_sends_every_optional_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .and(session_cookie())
        .and(query_param("start", "2026-01-01T00:00:00Z"))
        .and(query_param("end", "2026-01-02T00:00:00Z"))
        .and(query_param("busy_threshold", "5"))
        .and(query_param("eero_id", "eero-0001"))
        .and(query_param("band", "band_5GHz_low"))
        .and(query_param("granularity", "60"))
        .and(query_param("gap_data_placeholder", "-1"))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions {
        busy_threshold: Some(5),
        eero_id: Some("eero-0001"),
        band: Some("band_5GHz_low"),
        granularity: Some(60),
        gap_data_placeholder: Some("-1"),
    };
    let env = api(&mock)
        .get_channel_utilization(
            "network-0001",
            "2026-01-01T00:00:00Z",
            "2026-01-02T00:00:00Z",
            &options,
            None,
        )
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_accepts_every_valid_band() -> anyhow::Result<()> {
    for band in rusteero::endpoints::events::CHANNEL_UTILIZATION_BANDS {
        let mock = MockEero::start().await;
        let body = json!({ "meta": { "code": 200 }, "data": {} });
        Mock::given(method("GET"))
            .and(path("/2.2/networks/network-0001/channel_utilization"))
            .and(session_cookie())
            .and(query_param("band", *band))
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
            .expect(1)
            .mount(&mock.server)
            .await;

        let options = GetChannelUtilizationOptions {
            band: Some(*band),
            ..GetChannelUtilizationOptions::default()
        };
        api(&mock)
            .get_channel_utilization("network-0001", "s", "e", &options, None)
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_rejects_an_invalid_band_with_zero_requests() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions {
        band: Some("not_a_real_band"),
        ..GetChannelUtilizationOptions::default()
    };
    let err = api(&mock)
        .get_channel_utilization("network-0001", "s", "e", &options, None)
        .await
        .expect_err("an unrecognised band must be rejected locally");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "band");
    assert_eq!(
        message,
        "must be one of ('band_2_4GHz', 'band_5GHz_low', 'band_5GHz_high', 'band_5GHz_full', \
         'band_6GHz'), got 'not_a_real_band'"
    );
    Ok(())
}

/// Ported from `events.py:216-223`: `busy_threshold` is validated before `band`, which is
/// validated before `granularity`. A caller passing an invalid `busy_threshold` alongside an
/// invalid `band` must see the `busy_threshold` error, not the `band` one.
#[tokio::test]
async fn get_channel_utilization_validates_busy_threshold_before_band() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions {
        busy_threshold: Some(0),
        band: Some("not_a_real_band"),
        ..GetChannelUtilizationOptions::default()
    };
    let err = api(&mock)
        .get_channel_utilization("network-0001", "s", "e", &options, None)
        .await
        .expect_err("busy_threshold=0 must be rejected before band is even checked");
    assert!(matches!(err, Error::Validation { field, .. } if field == "busy_threshold"));
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_rejects_zero_granularity_with_zero_requests() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions {
        granularity: Some(0),
        ..GetChannelUtilizationOptions::default()
    };
    let err = api(&mock)
        .get_channel_utilization("network-0001", "s", "e", &options, None)
        .await
        .expect_err("granularity=0 must be rejected locally");
    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "granularity");
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_rejects_zero_busy_threshold_with_zero_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/channel_utilization"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let options = GetChannelUtilizationOptions {
        busy_threshold: Some(0),
        ..GetChannelUtilizationOptions::default()
    };
    let err = api(&mock)
        .get_channel_utilization("network-0001", "s", "e", &options, None)
        .await
        .expect_err("busy_threshold=0 must be rejected locally");
    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "busy_threshold");
    Ok(())
}

#[tokio::test]
async fn get_channel_utilization_prefers_a_parent_supplied_self_url_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/channel_utilization"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let options = GetChannelUtilizationOptions::default();
    let env = api(&mock)
        .get_channel_utilization("network-0001", "s", "e", &options, Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}
