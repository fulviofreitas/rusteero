//! HTTP integration tests for `EerosApi`, pinned against `eero-api src/eero/api/eeros.py` at
//! v8.0.4. See `src/endpoints/eeros.rs`'s module docs for the `network_id`-parameter divergence,
//! the validate-not-clamp brightness change, the `set_led`/`set_led_brightness` wire-format
//! change, and the nightlight-discovery shape these tests exercise directly.

mod common;

use std::sync::Arc;

use rusteero::endpoints::eeros::EerosApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds an `EerosApi` pointed at `mock`, authenticated with [`TEST_TOKEN`].
fn eeros_api(mock: &MockEero) -> EerosApi {
    EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Form-urlencodes `pairs` exactly like `Transport::apply_body`'s `RequestBody::Form` branch, so
/// tests never hand-guess the encoded string.
fn form_encode(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

// ===================== get_eeros =====================

#[tokio::test]
async fn get_eeros_hits_the_networks_nested_path_with_both_credentials() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eeros("network-0001", None).await?;

    assert_eq!(env.into_value(), fixture_json("eeros.json"));
    Ok(())
}

#[tokio::test]
async fn get_eeros_prefers_a_parent_supplied_eeros_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/eeros"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"eeros": "/2.3/networks/network-0001/eeros"}});
    let api = eeros_api(&mock);
    api.get_eeros("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== get_eero =====================

// The dedicated `/2.2/` regression test the task brief calls out explicitly: unlike
// `DevicesApi`'s nickname/pause PUTs, `EerosApi` never switches to the `/2.3` base for this
// resource by template, and `get_eero` is never nested under `networks/`.
#[tokio::test]
async fn get_eero_hits_the_top_level_eeros_path_not_nested_under_networks() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eero("eero-0001", None).await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

#[tokio::test]
async fn get_eero_prefers_the_parents_own_self_url_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/eeros/eero-0001"});
    let api = eeros_api(&mock);
    api.get_eero("eero-0001", Some(&parent)).await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_404_maps_to_error_not_found() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/missing-eero"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such eero"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let err = api
        .get_eero("missing-eero", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    Ok(())
}

// ===================== get_led_status =====================

#[tokio::test]
async fn get_led_status_hits_the_same_path_as_get_eero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_led_status("eero-0001", None).await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

#[tokio::test]
async fn get_led_status_prefers_the_parents_own_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/eeros/eero-0001"});
    let api = eeros_api(&mock);
    api.get_led_status("eero-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== set_location =====================

#[tokio::test]
async fn set_location_sends_a_form_encoded_location() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(form_encode(&[("location", "Office")])))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_location("eero-0001", "Office", None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_location_prefers_the_parents_own_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/eeros/eero-0001"});
    let api = eeros_api(&mock);
    api.set_location("eero-0001", "Office", Some(&parent))
        .await?;
    Ok(())
}

// ===================== reboot_eero =====================

#[tokio::test]
async fn reboot_eero_posts_the_literal_empty_json_string_to_the_reboot_path() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.reboot_eero("eero-0001", None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn reboot_eero_prefers_a_parent_supplied_reboot_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.3/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"reboot": "/2.3/eeros/eero-0001/reboot"}});
    let api = eeros_api(&mock);
    api.reboot_eero("eero-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== set_led =====================

#[tokio::test]
async fn set_led_true_sends_a_form_encoded_led_on_true() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001/led"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(form_encode(&[("led_on", "true")])))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led("eero-0001", true, None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_false_sends_a_form_encoded_led_on_false() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001/led"))
        .and(session_cookie())
        .and(body_string(form_encode(&[("led_on", "false")])))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led("eero-0001", false, None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_prefers_a_parent_supplied_led_action_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/eeros/eero-0001/led"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"led_action": "/2.3/eeros/eero-0001/led"}});
    let api = eeros_api(&mock);
    api.set_led("eero-0001", true, Some(&parent)).await?;
    Ok(())
}

// ===================== set_led_brightness =====================

#[tokio::test]
async fn set_led_brightness_sends_a_form_encoded_stringified_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001/led"))
        .and(session_cookie())
        .and(body_string(form_encode(&[("led_brightness", "42")])))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.set_led_brightness("eero-0001", 42, None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_prefers_a_parent_supplied_led_action_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/eeros/eero-0001/led"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(form_encode(&[("led_brightness", "42")])))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"led_action": "/2.3/eeros/eero-0001/led"}});
    let api = eeros_api(&mock);
    api.set_led_brightness("eero-0001", 42, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_rejects_a_negative_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .set_led_brightness("eero-0001", -1, None)
        .await
        .expect_err("out-of-range brightness must be rejected, not clamped");
    assert!(matches!(err, Error::Validation { field, .. } if field == "brightness"));
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_rejects_a_value_over_100() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .set_led_brightness("eero-0001", 101, None)
        .await
        .expect_err("out-of-range brightness must be rejected, not clamped");
    assert!(matches!(err, Error::Validation { field, .. } if field == "brightness"));
    Ok(())
}

// ===================== get_connections =====================

#[tokio::test]
async fn get_connections_uses_the_default_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001/connections"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"meta\":{},\"data\":[]}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    api.get_connections("eero-0001", None).await?;
    Ok(())
}

#[tokio::test]
async fn get_connections_prefers_a_parent_supplied_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.3/eeros/eero-0001/connections"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"meta\":{},\"data\":[]}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"connections": "/2.3/eeros/eero-0001/connections"}});
    let api = eeros_api(&mock);
    api.get_connections("eero-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== get_nightlight =====================

#[tokio::test]
async fn get_nightlight_uses_the_parents_nightlight_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"enabled\":true}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"nightlight": {"url": "/2.2/eeros/eero-beacon/nightlight"}});
    let api = eeros_api(&mock);
    let env = api.get_nightlight("eero-beacon", Some(&parent)).await?;
    assert_eq!(env.into_value(), json!({"enabled": true}));
    Ok(())
}

#[tokio::test]
async fn get_nightlight_discovers_the_url_via_one_extra_get() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-beacon"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "{\"meta\":{},\"data\":{\"serial\":\"ABC123\",\"nightlight\":{\"url\":\"/2.2/eeros/eero-beacon/nightlight\"}}}",
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"enabled\":false}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_nightlight("eero-beacon", None).await?;
    assert_eq!(env.into_value(), json!({"enabled": false}));
    Ok(())
}

#[tokio::test]
async fn get_nightlight_raises_feature_unavailable_when_absent_everywhere() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"meta\":{},\"data\":{\"serial\":\"ABC123\"}}"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let err = api
        .get_nightlight("eero-0001", None)
        .await
        .expect_err("no nightlight.url anywhere must raise FeatureUnavailable");
    assert!(matches!(err, Error::FeatureUnavailable { .. }));
    Ok(())
}

// ===================== set_nightlight =====================

#[tokio::test]
async fn set_nightlight_sends_only_the_supplied_fields_top_level() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": true })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"nightlight": {"url": "/2.2/eeros/eero-beacon/nightlight"}});
    let api = eeros_api(&mock);
    let env = api
        .set_nightlight("eero-beacon", Some(true), None, None, Some(&parent))
        .await?;
    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn set_nightlight_forwards_an_opaque_schedule_object_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let schedule = json!({"days": ["MON", "TUE"], "start": "20:00"});
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(body_json(json!({ "schedule": schedule })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"nightlight": {"url": "/2.2/eeros/eero-beacon/nightlight"}});
    let api = eeros_api(&mock);
    api.set_nightlight("eero-beacon", None, None, Some(schedule), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_nightlight_requires_at_least_one_field_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .set_nightlight("eero-beacon", None, None, None, None)
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

#[tokio::test]
async fn set_nightlight_rejects_out_of_range_brightness_before_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .set_nightlight("eero-beacon", None, Some(101), None, None)
        .await
        .expect_err("out-of-range brightness must be rejected before any request is sent");

    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "brightness_percentage");

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== set_nightlight_brightness / set_nightlight_schedule =====================

#[tokio::test]
async fn set_nightlight_brightness_delegates_with_only_the_brightness_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(body_json(json!({ "brightness_percentage": 55 })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"nightlight": {"url": "/2.2/eeros/eero-beacon/nightlight"}});
    let api = eeros_api(&mock);
    api.set_nightlight_brightness("eero-beacon", 55, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_nightlight_schedule_delegates_with_only_the_schedule_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let schedule = json!({"days": ["SAT"]});
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-beacon/nightlight"))
        .and(session_cookie())
        .and(body_json(json!({ "schedule": schedule })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"nightlight": {"url": "/2.2/eeros/eero-beacon/nightlight"}});
    let api = eeros_api(&mock);
    api.set_nightlight_schedule("eero-beacon", schedule, Some(&parent))
        .await?;
    Ok(())
}

// ===================== node_action =====================

#[tokio::test]
async fn node_action_sends_json_for_power_cycle_all_ports() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/action"))
        .and(session_cookie())
        .and(body_json(json!({ "action": "POWER_CYCLE_ALL_PORTS" })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    api.node_action("eero-0001", "POWER_CYCLE_ALL_PORTS", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn node_action_sends_json_for_power_cycle_all_ports_and_reboot() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/action"))
        .and(session_cookie())
        .and(body_json(
            json!({ "action": "POWER_CYCLE_ALL_PORTS_AND_REBOOT" }),
        ))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    api.node_action("eero-0001", "POWER_CYCLE_ALL_PORTS_AND_REBOOT", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn node_action_prefers_a_parent_supplied_action_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.3/eeros/eero-0001/action"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"action": "/2.3/eeros/eero-0001/action"}});
    let api = eeros_api(&mock);
    api.node_action("eero-0001", "POWER_CYCLE_ALL_PORTS", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn node_action_rejects_an_unrecognised_action() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .node_action("eero-0001", "NOT_A_REAL_ACTION", None)
        .await
        .expect_err("an unrecognised action must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "action"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== port_action =====================

#[tokio::test]
async fn port_action_sends_json_to_the_ports_id_action_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/ports/1/action"))
        .and(session_cookie())
        .and(body_json(json!({ "action": "ENABLE_DATA" })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    api.port_action("eero-0001", 1, "ENABLE_DATA").await?;
    Ok(())
}

#[tokio::test]
async fn port_action_rejects_an_unrecognised_action() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    let err = api
        .port_action("eero-0001", 1, "NOT_A_REAL_ACTION")
        .await
        .expect_err("an unrecognised action must be rejected before any request is sent");
    assert!(matches!(err, Error::Validation { field, .. } if field == "action"));
    Ok(())
}

// ===================== led_cycle =====================

#[tokio::test]
async fn led_cycle_sends_a_form_encoded_body_with_repeated_colors() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let expected = form_encode(&[
        ("colors[]", "red"),
        ("colors[]", "green"),
        ("duration", "5"),
        ("time_per_color", "2"),
    ]);
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/SERIAL123/led_cycle"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(expected))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let colors = vec!["red".to_owned(), "green".to_owned()];
    api.led_cycle("SERIAL123", &colors, 5, 2).await?;
    Ok(())
}

// ===================== nightlight_override =====================

#[tokio::test]
async fn nightlight_override_sends_a_form_encoded_value() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/nightlight/override"))
        .and(session_cookie())
        .and(body_string(form_encode(&[("brightness_percentage", "33")])))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    api.nightlight_override("eero-0001", 33).await?;
    Ok(())
}

#[tokio::test]
async fn nightlight_override_rejects_out_of_range_values() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = eeros_api(&mock);
    for value in [-1, 101] {
        let err = api
            .nightlight_override("eero-0001", value)
            .await
            .expect_err("out-of-range brightness must be rejected");
        assert!(matches!(err, Error::Validation { field, .. } if field == "brightness_percentage"));
    }
    Ok(())
}

// ===================== get_eero_support =====================

#[tokio::test]
async fn get_eero_support_returns_the_raw_response() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/SERIAL123/support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"meta\":{},\"data\":{\"diagnostics\":[]}}"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eero_support("SERIAL123").await?;
    // Ported from `EerosAPI.get_eero_support` (`eeros.py:836-863`): returns the raw, unmodified
    // `{"meta": ..., "data": ...}` envelope, like every other endpoint method in this crate --
    // never the stripped `data` object alone.
    assert_eq!(
        env.into_value(),
        json!({"meta": {}, "data": {"diagnostics": []}})
    );
    Ok(())
}

#[tokio::test]
async fn get_eero_support_raises_not_found_on_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/SERIAL123/support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no support info"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let err = api
        .get_eero_support("SERIAL123")
        .await
        .expect_err("a 404 must surface as Error::NotFound");
    assert!(matches!(err, Error::NotFound { status: 404, .. }));
    Ok(())
}
