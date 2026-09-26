//! `Client` integration suite for the `eeros` domain: cache reads/invalidation, the
//! `_eero_parent_kwargs`/`_network_parent_kwargs` forwarding this crate reproduces from
//! `eero-api src/eero/client.py`, and the `get_eero`/`led_cycle`/`get_eero_support`/`port_action`/
//! `nightlight_override` quirks recorded in `.claude/tasks/briefs/v8/g2-eeros.md` §3.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

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

// ===================== get_eeros: cache read =====================

#[tokio::test]
async fn get_eeros_serves_the_second_call_from_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let first = client.get_eeros(Some("network-0001"), false).await?;
    let second = client.get_eeros(Some("network-0001"), false).await?;
    assert_eq!(first.into_value(), second.into_value());
    Ok(())
}

// ===================== get_eero: never cached =====================

#[tokio::test]
async fn get_eero_is_never_served_from_cache_even_with_refresh_cache_false() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_eero("eero-0001", Some("network-0001"), false)
        .await?;
    client
        .get_eero("eero-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_forwards_a_cached_eero_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    // `eeros.json`'s first entry publishes its own `url`
    // (`/2.2/networks/network-0001/eeros/eero-0001`), which `find_by_id_or_url` matches on the
    // trailing `eero-0001` segment even though the entry carries no bare `id` field. `get_eero`
    // then prefers that entry's `self_url` over the bare-id template (`resolve_self_preferred`),
    // so seeding the eeros cache first changes the request path — proving `eero_parent` really is
    // forwarded, not just looked up harmlessly. The bare-id template
    // (`/2.2/eeros/eero-0001`) must NOT be hit once the cache is warm.
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_eero("eero-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

// ===================== reboot_eero: invalidation + parent =====================

#[tokio::test]
async fn reboot_eero_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .reboot_eero("eero-0001", Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== set_led / set_led_brightness =====================

#[tokio::test]
async fn set_led_reaches_the_led_endpoint_and_invalidates_eeros() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001/led"))
        .and(session_cookie())
        // `EerosApi::set_led` sends a form-encoded body (`RequestBody::Form`), not JSON --
        // `body_json` would never match a `led_on=true` wire body, silently leaving this mock
        // unhit and the request unmatched.
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("led_on=true"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .set_led("eero-0001", true, Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_led_brightness_rejects_out_of_range_without_invalidating_the_cache()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let first = client.get_eeros(Some("network-0001"), false).await?;

    let err = client
        .set_led_brightness("eero-0001", 999, Some("network-0001"))
        .await
        .expect_err("out-of-range brightness must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "brightness"));

    // Still cached: no second GET was registered above, so a second call proves the cache was
    // not invalidated by the failed write.
    let second = client.get_eeros(Some("network-0001"), false).await?;
    assert_eq!(first.into_value(), second.into_value());
    Ok(())
}

// ===================== node_action / port_action / nightlight_override =====================

#[tokio::test]
async fn node_action_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/action"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .node_action("eero-0001", "POWER_CYCLE_ALL_PORTS", Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

// `port_action` resolves `network_id` only to invalidate the cache — it is never forwarded to
// the domain call, which has no `network_id` parameter at all (structurally guaranteed by
// `EerosApi::port_action`'s own signature; this test exercises the invalidation half of the
// quirk).
#[tokio::test]
async fn port_action_invalidates_the_eeros_bucket_without_a_network_id_param() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/ports/1/action"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .port_action("eero-0001", 1, "ENABLE_DATA", Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn nightlight_override_invalidates_the_eeros_bucket_without_a_network_id_param()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/nightlight/override"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .nightlight_override("eero-0001", 42, Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== led_cycle / get_eero_support: no network_id at all =====================

#[tokio::test]
async fn led_cycle_takes_no_network_id_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/SERIAL123/led_cycle"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let colors = vec!["red".to_owned()];
    client.led_cycle("SERIAL123", &colors, 5, 2).await?;
    Ok(())
}

#[tokio::test]
async fn get_eero_support_takes_no_network_id_parameter() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/SERIAL123/support"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_eero_support("SERIAL123").await?;
    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}
