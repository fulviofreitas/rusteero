//! `Client` integration suite for `DataUsageApi`'s pass-throughs at `v8.4.1`.
//!
//! None of these twelve methods is cached; `set_data_usage_report_settings` is the only write
//! and is the only one that invalidates anything.

mod common;

use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, Request, ResponseTemplate, Times};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::envelope::Envelope;
use rusteero::error::Error;
use rusteero::transport::RequestTimeout;

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
async fn get_data_usage_forwards_every_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(user_token_header())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("timezone", "UTC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage("s", "e", "daily", Some("UTC"), None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_data_usage_breakdown_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/data_usage/breakdown"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage_breakdown("s", "e", None, None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_data_usage_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/data_usage/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices_data_usage("s", "e", None, None, None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_data_usage_hits_the_per_device_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/devices/aa:bb:cc",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_device_data_usage(
            "aa:bb:cc",
            "s",
            "e",
            "daily",
            None,
            None,
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eeros_data_usage_summary_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001/data_usage/eeros/summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_eeros_data_usage_summary("s", "e", "daily", None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_data_usage_hits_the_per_eero_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_eero_data_usage(
            "eero-0001",
            "s",
            "e",
            "daily",
            None,
            None,
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_profile_data_usage_hits_the_per_profile_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/profiles/profile-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile_data_usage(
            "profile-0001",
            "s",
            "e",
            "daily",
            None,
            None,
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_devices_data_usage_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/devices",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_unprofiled_devices_data_usage("s", "e", None, None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_unprofiled_data_usage_summary_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/unprofiled/summary",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_unprofiled_data_usage_summary("s", "e", "daily", None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_data_usage_report_settings_hits_the_expected_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage_report_settings(Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_data_usage_report_settings_invalidates_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path("/2.2/networks/network-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({"meta": {"code": 200}, "data": {"id": "network-0001"}}).to_string(),
        ))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({ "cadence": "daily", "notification_day": "monday" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_data_usage_report_settings("daily", "monday", Some("network-0001"))
        .await?;
    // `network[{nid}]` was invalidated by the write above, so this second read must hit the
    // network again.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== get_eeros_data_usage =====================

#[tokio::test]
async fn get_eeros_data_usage_hits_the_eeros_path_with_every_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": [{"eero": "eero-0001"}]});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros"))
        .and(session_cookie())
        .and(user_token_header())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "hourly"))
        .and(query_param("timezone", "Europe/London"))
        .and(|request: &Request| request.url.query_pairs().count() == 4 && request.body.is_empty())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // Neither the summary nor a single-eero path is the target.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros/summary"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_eeros_data_usage(
            "s",
            "e",
            "hourly",
            Some("Europe/London"),
            None,
            Some("network-0001"),
        )
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_eeros_data_usage_is_never_cached_and_invalidates_nothing() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros"))
        .and(session_cookie())
        .and(user_token_header())
        // A truthy envelope: `Cache::get` never serves a falsy one (an empty object is a miss), so
        // an empty body would let a regression that cached this response still reach the mock
        // twice and pass `expect(2)`.
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "meta": {"code": 200},
            "data": [{"eero": "eero-0001"}],
        })))
        .expect(2)
        .mount(&mock.server)
        .await;
    // A primed network entry survives both reads: it is fetched once, then served from cache.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    for _ in 0..2 {
        client
            .get_eeros_data_usage("s", "e", "daily", None, None, Some("network-0001"))
            .await?;
    }
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

/// The facade forwards no `parent=`, even when a network envelope is cached.
#[tokio::test]
async fn get_eeros_data_usage_does_not_forward_the_cached_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "meta": {"code": 200},
            "data": {"id": "network-0001", "url": "/2.4/networks/network-0001"},
        })))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/data_usage/eeros"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .get_eeros_data_usage("s", "e", "daily", None, None, Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eeros_data_usage_resolves_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .get_eeros_data_usage("s", "e", "daily", None, None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}

#[tokio::test]
async fn get_eeros_data_usage_rejects_an_invalid_cadence_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .get_eeros_data_usage("s", "e", "weekly", None, None, Some("network-0001"))
        .await
        .expect_err("an invalid cadence must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}

// ===================== per-call timeout =====================
//
// Real clock (the crate's timeouts and a paused clock do not mix with a real HTTP round trip).
// `ClientBuilder` has no timeout setters, so the short default comes from a caller-supplied
// `reqwest` builder with a 250 ms total timeout; a per-call `RequestTimeout` replaces it. The
// "must time out" mocks accept zero or one hit: the total deadline also covers connecting and
// sending, so on a loaded machine a call can expire before its request reaches the mock server.
// It still returns `Error::Timeout`, which is all those tests assert.

const SLOW: Duration = Duration::from_millis(2000);
const SHORT: Duration = Duration::from_millis(250);
const GENEROUS: Duration = Duration::from_secs(6);

/// A [`Client`] whose underlying HTTP client times out after 250 ms.
async fn short_timeout_client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .http_builder(reqwest::Client::builder().timeout(SHORT))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

/// The paths of the ten time-windowed reads, in declaration order.
const TIME_WINDOWED_PATHS: [&str; 10] = [
    "/2.2/networks/network-0001/data_usage",
    "/2.2/networks/network-0001/data_usage/breakdown",
    "/2.2/networks/network-0001/data_usage/devices",
    "/2.2/networks/network-0001/data_usage/devices/aa:bb:cc",
    "/2.2/networks/network-0001/data_usage/eeros",
    "/2.2/networks/network-0001/data_usage/eeros/summary",
    "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
    "/2.2/networks/network-0001/data_usage/profiles/profile-0001",
    "/2.2/networks/network-0001/data_usage/unprofiled/devices",
    "/2.2/networks/network-0001/data_usage/unprofiled/summary",
];

/// Mounts one authenticated, [`SLOW`] `GET` mock per time-windowed path, each expecting `hits`
/// requests carrying `start`/`end` query parameters.
async fn mount_slow_time_windowed_reads(mock: &MockEero, hits: Times) {
    for route in TIME_WINDOWED_PATHS {
        Mock::given(method("GET"))
            .and(path(route))
            .and(session_cookie())
            .and(user_token_header())
            .and(query_param("start", "s"))
            .and(query_param("end", "e"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"meta": {"code": 200}, "data": {}}))
                    .set_delay(SLOW),
            )
            .expect(hits.clone())
            .mount(&mock.server)
            .await;
    }
}

/// Calls all ten time-windowed wrappers concurrently with `timeout`, in
/// [`TIME_WINDOWED_PATHS`] order.
async fn call_every_time_windowed_wrapper(
    client: &Client,
    timeout: Option<RequestTimeout>,
) -> [Result<Envelope, Error>; 10] {
    let nid = Some("network-0001");
    let (
        data_usage,
        breakdown,
        devices,
        device,
        eeros,
        eeros_summary,
        eero,
        profile,
        unprofiled_devices,
        unprofiled_summary,
    ) = tokio::join!(
        client.get_data_usage("s", "e", "daily", None, timeout, nid),
        client.get_data_usage_breakdown("s", "e", None, None, timeout, nid),
        client.get_devices_data_usage("s", "e", None, None, None, timeout, nid),
        client.get_device_data_usage("aa:bb:cc", "s", "e", "daily", None, timeout, nid),
        client.get_eeros_data_usage("s", "e", "daily", None, timeout, nid),
        client.get_eeros_data_usage_summary("s", "e", "daily", None, timeout, nid),
        client.get_eero_data_usage("eero-0001", "s", "e", "daily", None, timeout, nid),
        client.get_profile_data_usage("profile-0001", "s", "e", "daily", None, timeout, nid),
        client.get_unprofiled_devices_data_usage("s", "e", None, None, timeout, nid),
        client.get_unprofiled_data_usage_summary("s", "e", "daily", None, timeout, nid),
    );
    [
        data_usage,
        breakdown,
        devices,
        device,
        eeros,
        eeros_summary,
        eero,
        profile,
        unprofiled_devices,
        unprofiled_summary,
    ]
}

#[tokio::test]
async fn every_data_usage_wrapper_forwards_a_supplied_timeout() {
    let mock = MockEero::start().await;
    mount_slow_time_windowed_reads(&mock, Times::from(1)).await;
    let client = short_timeout_client(&mock).await;

    let results = Box::pin(call_every_time_windowed_wrapper(
        &client,
        Some(RequestTimeout::total(GENEROUS)),
    ))
    .await;
    for (route, result) in TIME_WINDOWED_PATHS.iter().zip(results) {
        assert!(result.is_ok(), "{route}: {result:?}");
    }
}

#[tokio::test]
async fn every_data_usage_wrapper_defaults_to_the_transport_timeout() {
    let mock = MockEero::start().await;
    mount_slow_time_windowed_reads(&mock, Times::from(0..=1)).await;
    let client = short_timeout_client(&mock).await;

    let results = Box::pin(call_every_time_windowed_wrapper(&client, None)).await;
    for (route, result) in TIME_WINDOWED_PATHS.iter().zip(results) {
        assert!(matches!(result, Err(Error::Timeout)), "{route}: {result:?}");
    }
}

#[tokio::test]
async fn get_data_usage_with_a_timeout_sends_the_same_query_and_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(user_token_header())
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param("timezone", "UTC"))
        // No extra query parameter (the timeout is client-side only) and no body.
        .and(|request: &Request| request.url.query_pairs().count() == 4 && request.body.is_empty())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_data_usage(
            "s",
            "e",
            "daily",
            Some("UTC"),
            Some(RequestTimeout::total(GENEROUS)),
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn report_settings_wrappers_take_no_timeout_and_keep_the_transport_default()
-> anyhow::Result<()> {
    // `get_data_usage_report_settings` / `set_data_usage_report_settings` keep their
    // pre-timeout arity (the tests above call them the same way) and run under the default.
    //
    // Each mock accepts zero or one hit for the reason given above the constants.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(SLOW),
        )
        .expect(0..=1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({ "cadence": "daily", "notification_day": "monday" }),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(SLOW),
        )
        .expect(0..=1)
        .mount(&mock.server)
        .await;
    let client = short_timeout_client(&mock).await;

    let err = client
        .get_data_usage_report_settings(Some("network-0001"))
        .await
        .expect_err("the 250 ms transport default applies");
    assert!(matches!(err, Error::Timeout), "{err:?}");
    let err = client
        .set_data_usage_report_settings("daily", "monday", Some("network-0001"))
        .await
        .expect_err("the 250 ms transport default applies");
    assert!(matches!(err, Error::Timeout), "{err:?}");
    Ok(())
}
