//! HTTP integration tests for `DataUsageApi` (`src/endpoints/data_usage.rs`) at `v8.0.4`.
//!
//! Every method in this family sends `start`/`end`/`cadence`/`timezone` as *query* parameters,
//! never a request body — proven both by `query_param` matchers and, on the two-path tests, an
//! `.expect(0)` mock on the sibling path so a bug that renders the wrong path fails loudly.

mod common;

use std::sync::Arc;
use std::time::Duration;

use rstest::rstest;
use rusteero::auth::Session;
use rusteero::endpoints::data_usage::DataUsageApi;
use rusteero::envelope::Envelope;
use rusteero::error::Error;
use rusteero::transport::{RequestTimeout, Transport};
use serde_json::{Value, json};
use wiremock::matchers::{
    body_json, method, path, path_regex, query_param, query_param_is_missing,
};
use wiremock::{Mock, Request, ResponseTemplate, Times};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

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
        .and(user_token_header())
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_data_usage("network-0001", "s", "e", "daily", None, None, None)
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
            .get_data_usage("network-0001", "s", "e", bad, None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_data_usage("network-0001", "s", "e", "daily", None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_breakdown("network-0001", "s", "e", None, None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_breakdown_cadence_included_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/breakdown"))
        .and(query_param("cadence", "hourly"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_breakdown("network-0001", "s", "e", Some("hourly"), None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_breakdown_invalid_cadence_rejected() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_breakdown("network-0001", "s", "e", Some("weekly"), None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_devices_usage("network-0001", "s", "e", None, None, None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_unprofiled_devices("network-0001", "s", "e", None, None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_device_usage(
        "network-0001",
        "aa:bb:cc",
        "s",
        "e",
        "daily",
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn get_device_usage_empty_child_id_rejected() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_device_usage("network-0001", "", "s", "e", "daily", None, None, None)
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
        .get_device_usage(
            "network-0001",
            "aa{bb}cc",
            "s",
            "e",
            "daily",
            None,
            None,
            None,
        )
        .await
        .expect_err("a brace-carrying child id must be rejected, not silently substituted");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

/// Ported from `data_usage.py:317`: `_validate_child_id(device_mac)` is embedded in the
/// f-string argument passed to `_get_usage`, so it is evaluated -- and can raise -- before
/// `_get_usage`'s own `cadence` check ever runs. An invalid child id
/// alongside an invalid cadence must surface the `id` error, not the `cadence` one.
#[tokio::test]
async fn get_device_usage_validates_child_id_before_cadence() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_device_usage("network-0001", "", "s", "e", "weekly", None, None, None)
        .await
        .expect_err("both id and cadence are invalid; id must be checked first");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

#[tokio::test]
async fn get_eero_usage_validates_child_id_before_cadence() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_eero_usage("network-0001", "", "s", "e", "weekly", None, None, None)
        .await
        .expect_err("both id and cadence are invalid; id must be checked first");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

#[tokio::test]
async fn get_profile_usage_validates_child_id_before_cadence() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let err = api
        .get_profile_usage("network-0001", "", "s", "e", "weekly", None, None, None)
        .await
        .expect_err("both id and cadence are invalid; id must be checked first");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id"));
    Ok(())
}

#[tokio::test]
async fn get_eeros_summary_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros/summary"))
        .and(query_param("cadence", "daily"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_eeros_summary("network-0001", "s", "e", "daily", None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eeros_usage_url_and_params() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({"meta": {"code": 200}, "data": [{"eero": "eero-0001"}]});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros"))
        .and(query_param("start", "s"))
        .and(query_param("end", "e"))
        .and(query_param("cadence", "daily"))
        .and(query_param_is_missing("timezone"))
        .and(session_cookie())
        .and(user_token_header())
        // The parameters are sent in Python's order, with no body.
        .and(|request: &Request| {
            request.url.query() == Some("start=s&end=e&cadence=daily") && request.body.is_empty()
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;
    // The sibling paths must stay untouched: this is neither the summary nor a single eero.
    for sibling in [
        "/2.2/networks/network-0001/data_usage/eeros/summary",
        "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
    ] {
        Mock::given(method("GET"))
            .and(path(sibling))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
    }

    let api = data_usage_api(&mock);
    let env = api
        .get_eeros_usage("network-0001", "s", "e", "daily", None, None, None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_eeros_usage_includes_timezone_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage/eeros"))
        .and(query_param("cadence", "hourly"))
        .and(query_param("timezone", "UTC"))
        .and(session_cookie())
        .and(user_token_header())
        .and(|request: &Request| {
            request.url.query() == Some("start=s&end=e&cadence=hourly&timezone=UTC")
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_eeros_usage("network-0001", "s", "e", "hourly", Some("UTC"), None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eeros_usage_rejects_invalid_cadence_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let err = api
        .get_eeros_usage("network-0001", "s", "e", "weekly", None, None, None)
        .await
        .expect_err("an invalid cadence must be rejected before any request");
    assert!(
        matches!(
            &err,
            Error::Validation { field, message, .. }
                if field == "cadence"
                    && message == "must be one of ('daily', 'hourly'), got 'weekly'"
        ),
        "{err:?}"
    );
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn get_eeros_usage_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = DataUsageApi::new(Arc::new(mock.transport_anonymous()));
    let err = api
        .get_eeros_usage("network-0001", "s", "e", "daily", None, None, None)
        .await
        .expect_err("no session must fail before any request");
    assert!(err.is_auth_error());
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_eero_usage(
        "network-0001",
        "eero-0001",
        "s",
        "e",
        "hourly",
        None,
        None,
        None,
    )
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
        .and(session_cookie())
        .and(user_token_header())
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
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    api.get_unprofiled_summary("network-0001", "s", "e", "daily", None, None, None)
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
        .and(session_cookie())
        .and(user_token_header())
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
        .and(user_token_header())
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

// ===================== parent self-url resolution (shared table test) =====================

/// Registers, on `mock`, a `.expect(0)` trap on every `bare` path and a `.expect(1)` credentialed
/// success mock on every corresponding `resolved` path — the shared setup for the two
/// self-url-preference tests below.
async fn expect_self_url_wins(mock: &MockEero, bare_paths: &[&str], resolved_paths: &[&str]) {
    let ok = json!({"meta": {"code": 200}, "data": {}});
    for bare in bare_paths {
        Mock::given(method("GET"))
            .and(path(*bare))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
    }
    for resolved in resolved_paths {
        Mock::given(method("GET"))
            .and(path(*resolved))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_json(ok.clone()))
            .expect(1)
            .mount(&mock.server)
            .await;
    }
}

/// Shared test (part 1/2, split to stay under `clippy::too_many_lines`) for every
/// `DataUsageApi` method's `network_id_or_self_url` behaviour (`data_usage.py:97-153`'s
/// `_get_usage`, ported doc comment on `DataUsageApi::network_id_or_self_url`): a `parent`
/// carrying its own `url` field wins over the bare-`network_id` template. Every bare-id path is
/// registered with `.expect(0)` so a regression that ignores `parent` fails loudly instead of
/// silently matching the wrong mock, and every self-url-resolved path (on a distinct network id
/// and API version, `/2.4/networks/other-network`) is registered with `.expect(1)`.
#[tokio::test]
async fn network_and_device_level_methods_prefer_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"url": "/2.4/networks/other-network"});

    expect_self_url_wins(
        &mock,
        &[
            "/2.2/networks/network-0001/data_usage",
            "/2.2/networks/network-0001/data_usage/breakdown",
            "/2.2/networks/network-0001/data_usage/devices",
            "/2.2/networks/network-0001/data_usage/devices/dev-0001",
            "/2.2/networks/network-0001/data_usage/eeros",
            "/2.2/networks/network-0001/data_usage/eeros/summary",
            "/2.2/networks/network-0001/data_usage/eeros/eero-0001",
        ],
        &[
            "/2.4/networks/other-network/data_usage",
            "/2.4/networks/other-network/data_usage/breakdown",
            "/2.4/networks/other-network/data_usage/devices",
            "/2.4/networks/other-network/data_usage/devices/dev-0001",
            "/2.4/networks/other-network/data_usage/eeros",
            "/2.4/networks/other-network/data_usage/eeros/summary",
            "/2.4/networks/other-network/data_usage/eeros/eero-0001",
        ],
    )
    .await;

    let api = data_usage_api(&mock);
    api.get_data_usage("network-0001", "s", "e", "daily", None, None, Some(&parent))
        .await?;
    api.get_breakdown("network-0001", "s", "e", None, None, None, Some(&parent))
        .await?;
    api.get_devices_usage(
        "network-0001",
        "s",
        "e",
        None,
        None,
        None,
        None,
        Some(&parent),
    )
    .await?;
    api.get_device_usage(
        "network-0001",
        "dev-0001",
        "s",
        "e",
        "daily",
        None,
        None,
        Some(&parent),
    )
    .await?;
    api.get_eeros_usage("network-0001", "s", "e", "daily", None, None, Some(&parent))
        .await?;
    api.get_eeros_summary("network-0001", "s", "e", "daily", None, None, Some(&parent))
        .await?;
    api.get_eero_usage(
        "network-0001",
        "eero-0001",
        "s",
        "e",
        "daily",
        None,
        None,
        Some(&parent),
    )
    .await?;
    Ok(())
}

/// Shared test (part 2/2) — see
/// [`network_and_device_level_methods_prefer_parents_self_url`] for the full rationale.
#[tokio::test]
async fn profile_and_report_level_methods_prefer_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"url": "/2.4/networks/other-network"});
    let ok = json!({"meta": {"code": 200}, "data": {}});

    expect_self_url_wins(
        &mock,
        &[
            "/2.2/networks/network-0001/data_usage/profiles/profile-0001",
            "/2.2/networks/network-0001/data_usage/unprofiled/devices",
            "/2.2/networks/network-0001/data_usage/unprofiled/summary",
            "/2.2/networks/network-0001/data_usage/report_settings",
        ],
        &[
            "/2.4/networks/other-network/data_usage/profiles/profile-0001",
            "/2.4/networks/other-network/data_usage/unprofiled/devices",
            "/2.4/networks/other-network/data_usage/unprofiled/summary",
            "/2.4/networks/other-network/data_usage/report_settings",
        ],
    )
    .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/data_usage/report_settings",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/other-network/data_usage/report_settings",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok))
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
        Some(&parent),
    )
    .await?;
    api.get_unprofiled_devices("network-0001", "s", "e", None, None, None, Some(&parent))
        .await?;
    api.get_unprofiled_summary("network-0001", "s", "e", "daily", None, None, Some(&parent))
        .await?;
    api.get_report_settings("network-0001", Some(&parent))
        .await?;
    api.set_report_settings("network-0001", "daily", "monday", Some(&parent))
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

// ===================== published parent urls are not family-confined =====================
//
// A parent's published `url` is only shape-checked (`data_usage.py:156-157` resolves it through
// `resolve_network_url`, which returns it verbatim); it is never confined to `networks/{id}`.
// Only a caller-supplied `network_id` is.

/// The twelve calls this module makes, each paired with the path suffix it must reach below the
/// parent's published url, in the order [`call_every_method_with_parent`] makes them.
const PUBLISHED_URL_SUFFIXES: [(&str, &str); 12] = [
    ("GET", "/data_usage"),
    ("GET", "/data_usage/breakdown"),
    ("GET", "/data_usage/devices"),
    ("GET", "/data_usage/devices/d1"),
    ("GET", "/data_usage/eeros"),
    ("GET", "/data_usage/eeros/summary"),
    ("GET", "/data_usage/eeros/e1"),
    ("GET", "/data_usage/profiles/p1"),
    ("GET", "/data_usage/unprofiled/devices"),
    ("GET", "/data_usage/unprofiled/summary"),
    ("GET", "/data_usage/report_settings"),
    ("PUT", "/data_usage/report_settings"),
];

/// Calls every `DataUsageApi` method with `parent`, naming a different network than `network_id`
/// would resolve to, in [`PUBLISHED_URL_SUFFIXES`] order.
async fn call_every_method_with_parent(api: &DataUsageApi, parent: &Value) -> Result<(), Error> {
    let net = "network-0001";
    api.get_data_usage(net, "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_breakdown(net, "s", "e", None, None, None, Some(parent))
        .await?;
    api.get_devices_usage(net, "s", "e", None, None, None, None, Some(parent))
        .await?;
    api.get_device_usage(net, "d1", "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_eeros_usage(net, "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_eeros_summary(net, "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_eero_usage(net, "e1", "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_profile_usage(net, "p1", "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_unprofiled_devices(net, "s", "e", None, None, None, Some(parent))
        .await?;
    api.get_unprofiled_summary(net, "s", "e", "daily", None, None, Some(parent))
        .await?;
    api.get_report_settings(net, Some(parent)).await?;
    api.set_report_settings(net, "daily", "monday", Some(parent))
        .await?;
    Ok(())
}

/// Calls every method with a parent whose published `url` is `published_url` and expects each
/// request on `<base><suffix>`, the bare-id template for the caller's `network_id` never touched.
async fn assert_every_method_resolves_under(published_url: &str, base: &str) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let ok = json!({"meta": {"code": 200}, "data": {}});
    for (verb, suffix) in PUBLISHED_URL_SUFFIXES {
        Mock::given(method(verb))
            .and(path(format!("{base}{suffix}")))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_json(ok.clone()))
            .expect(1)
            .mount(&mock.server)
            .await;
    }
    // Nothing may fall back to the template built from the caller's `network_id`.
    Mock::given(path_regex("^/2\\.2/networks/network-0001/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let parent = json!({"url": published_url});
    call_every_method_with_parent(&api, &parent).await?;
    Ok(())
}

/// A parent whose published `url` is not `/<version>/networks/<id>`-shaped still resolves every
/// method under that url verbatim: `<published url><suffix>`, with the bare-id template for the
/// caller's `network_id` never touched.
#[rstest]
#[case::account("/2.2/account", "/2.2/account")]
#[case::another_family("/2.3/networks/other/eeros", "/2.3/networks/other/eeros")]
#[case::unknown_shape("/2.4/some/other/shape", "/2.4/some/other/shape")]
#[tokio::test]
async fn every_method_resolves_a_non_network_shaped_published_url_verbatim(
    #[case] published_url: &str,
    #[case] base: &str,
) -> anyhow::Result<()> {
    assert_every_method_resolves_under(published_url, base).await
}

/// Deliberate divergence from Python: `data_usage.py:157` joins `f"{base_url}/data_usage"` onto
/// the published url exactly as `self_url` returned it, and `self_url` keeps a trailing slash, so
/// `/2.4/some/other/shape/` yields `/2.4/some/other/shape//data_usage` there. This crate drops the
/// trailing slash before joining the suffix, so no request ever carries a doubled slash.
#[tokio::test]
async fn every_method_drops_a_trailing_slash_from_a_published_url_before_the_suffix()
-> anyhow::Result<()> {
    assert_every_method_resolves_under("/2.4/some/other/shape/", "/2.4/some/other/shape").await
}

/// The query string and the JSON body are exactly what they are for a network-shaped parent.
#[tokio::test]
async fn a_non_network_shaped_published_url_keeps_the_query_and_body_shapes() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account/data_usage"))
        .and(session_cookie())
        .and(user_token_header())
        .and(|request: &Request| {
            request.url.query() == Some("start=s&end=e&cadence=daily&timezone=UTC")
                && request.body.is_empty()
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/data_usage/report_settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({"cadence": "hourly", "notification_day": "friday"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let parent = json!({"url": "/2.2/account"});
    api.get_data_usage(
        "network-0001",
        "s",
        "e",
        "daily",
        Some("UTC"),
        None,
        Some(&parent),
    )
    .await?;
    api.set_report_settings("network-0001", "hourly", "friday", Some(&parent))
        .await?;
    Ok(())
}

/// A published url is still shape-checked (field `link`), before any request.
#[rstest]
#[case::query("/2.2/networks/network-0001?x=1", "must not carry a query or fragment")]
#[case::absolute(
    "https://elsewhere.invalid/2.2/networks/network-0001",
    "must be a host-relative API path"
)]
#[tokio::test]
async fn a_malformed_published_url_is_still_rejected_before_any_request(
    #[case] published_url: &str,
    #[case] message: &str,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = data_usage_api(&mock);
    let parent = json!({"url": published_url});

    let err = api
        .get_eeros_usage("network-0001", "s", "e", "daily", None, None, Some(&parent))
        .await
        .expect_err("a malformed published url must be rejected before any request");
    assert!(
        matches!(
            &err,
            Error::Validation { field, message: got, .. } if field == "link" && got == message
        ),
        "{err:?}"
    );
    assert!(
        mock.server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

/// Only a caller-supplied `network_id` (the template fallback, no parent) is confined to
/// `networks/{id}`; the confinement itself is `links::resource_url`'s.
#[tokio::test]
async fn a_caller_supplied_network_outside_the_network_family_is_rejected() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let err = api
        .get_data_usage("/2.2/account", "s", "e", "daily", None, None, None)
        .await
        .expect_err("a caller-supplied non-network path must be rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "id_or_url"));
    Ok(())
}

// ===================== per-call timeout =====================
//
// Real clock (the crate's timeouts and a paused clock do not mix with a real HTTP round trip):
// every response is held back 2 s, eight times the 250 ms default these tests configure and
// well inside the 6 s budget they supply per call. The "must time out" mocks accept zero or one
// hit: the client's total deadline also covers connecting and sending, so on a loaded machine a
// call can expire before its request reaches the mock server. It still returns `Error::Timeout`,
// which is all those tests assert; that every path is reached, with credentials and query, is
// pinned by the generous-timeout test.

const SLOW: Duration = Duration::from_millis(2000);
const SHORT: Duration = Duration::from_millis(250);
const GENEROUS: Duration = Duration::from_secs(6);

/// A `DataUsageApi` whose transport times out after 250 ms (total and read).
fn short_timeout_api(mock: &MockEero) -> DataUsageApi {
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .timeout(SHORT)
        .read_timeout(SHORT)
        .build()
        .expect("a MockServer's own URI is always a valid base URL");
    DataUsageApi::new(Arc::new(transport))
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

/// Calls all ten time-windowed reads concurrently with `timeout`, in [`TIME_WINDOWED_PATHS`]
/// order.
async fn call_every_time_windowed_read(
    api: &DataUsageApi,
    timeout: Option<RequestTimeout>,
) -> [Result<Envelope, Error>; 10] {
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
        api.get_data_usage("network-0001", "s", "e", "daily", None, timeout, None),
        api.get_breakdown("network-0001", "s", "e", None, None, timeout, None),
        api.get_devices_usage("network-0001", "s", "e", None, None, None, timeout, None),
        api.get_device_usage(
            "network-0001",
            "aa:bb:cc",
            "s",
            "e",
            "daily",
            None,
            timeout,
            None
        ),
        api.get_eeros_usage("network-0001", "s", "e", "daily", None, timeout, None),
        api.get_eeros_summary("network-0001", "s", "e", "daily", None, timeout, None),
        api.get_eero_usage(
            "network-0001",
            "eero-0001",
            "s",
            "e",
            "daily",
            None,
            timeout,
            None
        ),
        api.get_profile_usage(
            "network-0001",
            "profile-0001",
            "s",
            "e",
            "daily",
            None,
            timeout,
            None
        ),
        api.get_unprofiled_devices("network-0001", "s", "e", None, None, timeout, None),
        api.get_unprofiled_summary("network-0001", "s", "e", "daily", None, timeout, None),
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
async fn every_time_windowed_read_honours_a_supplied_timeout() {
    let mock = MockEero::start().await;
    mount_slow_time_windowed_reads(&mock, Times::from(1)).await;
    let api = short_timeout_api(&mock);

    // `total` only: the 250 ms default read bound is replaced too, not merged back in.
    let results = Box::pin(call_every_time_windowed_read(
        &api,
        Some(RequestTimeout::total(GENEROUS)),
    ))
    .await;
    for (route, result) in TIME_WINDOWED_PATHS.iter().zip(results) {
        assert!(result.is_ok(), "{route}: {result:?}");
    }
}

#[tokio::test]
async fn every_time_windowed_read_keeps_the_default_without_a_timeout() {
    let mock = MockEero::start().await;
    mount_slow_time_windowed_reads(&mock, Times::from(0..=1)).await;
    let api = short_timeout_api(&mock);

    let results = Box::pin(call_every_time_windowed_read(&api, None)).await;
    for (route, result) in TIME_WINDOWED_PATHS.iter().zip(results) {
        assert!(matches!(result, Err(Error::Timeout)), "{route}: {result:?}");
    }
}

#[tokio::test]
async fn a_supplied_timeout_does_not_change_the_request_that_is_sent() -> anyhow::Result<()> {
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

    let api = data_usage_api(&mock);
    api.get_data_usage(
        "network-0001",
        "s",
        "e",
        "daily",
        Some("UTC"),
        Some(RequestTimeout::total(GENEROUS)),
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn validation_still_runs_before_any_request_when_a_timeout_is_supplied() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = data_usage_api(&mock);
    let err = api
        .get_data_usage(
            "network-0001",
            "s",
            "e",
            "weekly",
            None,
            Some(RequestTimeout::total(GENEROUS)),
            None,
        )
        .await
        .expect_err("an invalid cadence is rejected before any request");
    assert!(matches!(err, Error::Validation { field, .. } if field == "cadence"));
    Ok(())
}

#[tokio::test]
async fn report_settings_methods_take_no_timeout_and_keep_the_transport_default()
-> anyhow::Result<()> {
    // `get_report_settings` / `set_report_settings` keep their pre-timeout arity (this file's
    // other tests call them the same way) and run under the transport default.
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
    let api = short_timeout_api(&mock);

    let err = api
        .get_report_settings("network-0001", None)
        .await
        .expect_err("the 250 ms transport default applies");
    assert!(matches!(err, Error::Timeout), "{err:?}");
    let err = api
        .set_report_settings("network-0001", "daily", "monday", None)
        .await
        .expect_err("the 250 ms transport default applies");
    assert!(matches!(err, Error::Timeout), "{err:?}");
    Ok(())
}
