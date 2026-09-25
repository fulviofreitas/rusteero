//! HTTP integration tests for `EerosApi`'s read (`GET`) methods, pinned against
//! `eero-api src/eero/api/eeros.py`. See `src/endpoints/eeros.rs`'s module docs for the
//! `network_id`-parameter divergence these tests exercise indirectly (by never passing one to
//! `get_eero`/`get_led_status`/`get_nightlight`).

mod common;

use std::sync::Arc;

use rusteero::endpoints::eeros::EerosApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds an `EerosApi` pointed at `mock`, authenticated with [`TEST_TOKEN`].
fn eeros_api(mock: &MockEero) -> EerosApi {
    EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_eeros =====================

#[tokio::test]
async fn get_eeros_hits_the_networks_nested_path_with_the_session_cookie() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eeros("network-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eeros.json"));
    Ok(())
}

// ===================== get_eero =====================

// This is the test most likely to be wrong per the task brief: `get_eero` must NOT be nested
// under `networks/`, unlike `get_eeros` above. The exact path is asserted explicitly (both via
// the wiremock matcher, which only responds on that literal path, and via a second, independent
// full-URL assertion below) so a regression to `networks/{id}/eeros/{eero_id}` cannot pass
// silently.
#[tokio::test]
async fn get_eero_hits_the_top_level_eeros_path_not_nested_under_networks() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eero("eero-0001").await?;

    // Independent confirmation the rendered URL is exactly what was asserted via the wiremock
    // path matcher above — belt and braces for the one route this task brief calls out as most
    // likely to regress.
    let rendered = rusteero::routes::GET_EERO.render(&[("eero_id", "eero-0001")])?;
    assert_eq!(rendered.path(), "/2.2/eeros/eero-0001");

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== get_led_status =====================

#[tokio::test]
async fn get_led_status_hits_the_same_path_as_get_eero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_led_status("eero-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== get_nightlight =====================

#[tokio::test]
async fn get_nightlight_hits_the_same_path_as_get_eero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_nightlight("eero-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== 404 mapping =====================

#[tokio::test]
async fn get_eero_404_maps_to_error_api_with_status_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/missing-eero"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such eero"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let err = api
        .get_eero("missing-eero")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    Ok(())
}

// ===================== reboot_eero =====================
//
// Every test below pins the HTTP verb, the exact path and the exact JSON body via wiremock's
// `body_json` matcher plus an `.expect(n)` call count, per the crate's testing conventions. See
// `src/endpoints/eeros.rs`'s module docs for the `network_id`-parameter divergence, the
// brightness clamp and the empty-`set_nightlight` divergence these tests exercise directly.

#[tokio::test]
async fn reboot_eero_posts_an_empty_json_body_to_the_reboot_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.reboot_eero("eero-0001").await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== set_led =====================

#[tokio::test]
async fn set_led_true_sends_led_on_true() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_on": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led("eero-0001", true).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_false_sends_led_on_false() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_on": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led("eero-0001", false).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// This is the dedicated `/2.2/` regression test the task brief calls out explicitly: unlike
// `DevicesApi`'s nickname/pause PUTs, `EerosApi` never switches to the `/2.3` base. A sibling
// mock on the `/2.3` host with `.expect(0)` proves it — if a future edit ever routed this PUT to
// 2.3, that mock (not the 2.2 one) would receive the request and this test would fail loudly.
#[tokio::test]
async fn set_led_hits_v2_2_not_v2_3() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_on": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/eeros/eero-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led("eero-0001", true).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== set_led_brightness =====================

#[tokio::test]
async fn set_led_brightness_clamps_a_negative_value_to_zero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_brightness": 0 })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led_brightness("eero-0001", -5).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_clamps_a_value_over_100_to_100() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_brightness": 100 })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led_brightness("eero-0001", 150).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_within_range_passes_through_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "led_brightness": 42 })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led_brightness("eero-0001", 42).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== set_nightlight =====================

#[tokio::test]
async fn set_nightlight_with_every_field_builds_the_full_nested_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let expected = json!({
        "nightlight": {
            "enabled": true,
            "brightness": 42,
            "ambient_light_enabled": false,
            "schedule": {
                "enabled": true,
                "on": "20:00",
                "off": "06:00"
            }
        }
    });
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(expected))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api
        .set_nightlight(
            "eero-0001",
            Some(true),
            Some(42),
            Some(true),
            Some("20:00"),
            Some("06:00"),
            Some(false),
        )
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_nightlight_with_only_enabled_omits_every_other_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "nightlight": { "enabled": true } })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api
        .set_nightlight("eero-0001", Some(true), None, None, None, None, None)
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_nightlight_with_only_schedule_on_builds_a_schedule_only_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(
            json!({ "nightlight": { "schedule": { "on": "20:00" } } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api
        .set_nightlight("eero-0001", None, None, None, Some("20:00"), None, None)
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_nightlight_brightness_field_is_clamped_like_set_led_brightness() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "nightlight": { "brightness": 0 } })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api
        .set_nightlight("eero-0001", None, Some(-5), None, None, None, None)
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// This is the empty-call divergence from Python the task brief calls out explicitly
// (`eeros.py:269-272`): no `Mock` is registered at all here, so any code path that reaches the
// network would fail with a connection error rather than silently succeed — `received_requests`
// below double-checks the precondition fires before any I/O is attempted, not merely that no
// *registered* matcher happened to be hit.
#[tokio::test]
async fn set_nightlight_with_no_fields_is_a_validation_error_with_zero_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;

    let api = eeros_api(&mock);
    let err = api
        .set_nightlight("eero-0001", None, None, None, None, None, None)
        .await
        .expect_err("an empty call must be rejected before any request is sent");

    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "nightlight");

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== set_nightlight_brightness =====================

#[tokio::test]
async fn set_nightlight_brightness_delegates_with_only_the_brightness_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "nightlight": { "brightness": 100 } })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_nightlight_brightness("eero-0001", 150).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== set_nightlight_schedule =====================

#[tokio::test]
async fn set_nightlight_schedule_delegates_with_the_full_schedule_object() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "nightlight": {
                "schedule": {
                    "enabled": true,
                    "on": "20:00",
                    "off": "06:00"
                }
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api
        .set_nightlight_schedule("eero-0001", true, Some("20:00"), Some("06:00"))
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}
