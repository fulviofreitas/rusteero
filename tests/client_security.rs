//! `Client` integration suite for the `SecurityAPI` domain, v8.0.4: `network_id` resolution,
//! cache invalidation, and `parent=` forwarding from the cache (`Client::network_parent`).

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie, user_token_header};
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

#[tokio::test]
async fn set_wpa3_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_wpa3(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_wpa3_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_wpa3(true, Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_security_settings_forwards_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    // `get_security_settings` prefers the parent's own `url` (self_url), on `/2.4` in this
    // fixture — not the parent-supplied `resources.settings` link, which is irrelevant to a read.
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_security_settings(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn configure_security_no_longer_accepts_a_thread_argument_and_still_invalidates()
-> anyhow::Result<()> {
    // Regression proof for the v8.0.0 signature change: `configure_security` has no `thread`
    // parameter any more (see `src/client/security.rs`'s own docs) — this test compiling at all
    // is part of the proof; the invalidation behaviour is asserted the same way as every other
    // setter in this suite.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "wpa3": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .configure_security(Some(true), None, None, None, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_band_steering_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "band_steering": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_band_steering(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_upnp_forwards_the_cached_network_as_parent_and_invalidates_it() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "upnp": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_upnp(false, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_ipv6_forwards_the_cached_network_as_parent_and_invalidates_it() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_ipv6(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_mlo_mode_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/mlo_mode"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_mlo_mode("disabled", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_passpoint_enabled_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/passpoint/enabled"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_passpoint_enabled(true, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_proxied_nodes_forwards_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("dns_network_with_settings_link.json")),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/proxied_nodes"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_proxied_nodes(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn get_fast_transition_never_invalidates_and_set_fast_transition_does() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/fast_transition"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({"meta": {"code": 200}, "data": {}}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/fast_transition"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({"meta": {"code": 200}, "data": {}}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_fast_transition(Some("network-0001")).await?;
    client
        .set_fast_transition(true, Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== set_ipv6_upstream =====================

const SETTINGS_PATH: &str = "/2.2/networks/network-0001/settings";

fn ok_envelope() -> serde_json::Value {
    json!({"meta": {"code": 200}, "data": {}})
}

/// Mounts a `GET <path>` returning `body`, expecting exactly `times` requests.
async fn mount_get(mock: &MockEero, path_str: &str, body: String, times: u64) {
    Mock::given(method("GET"))
        .and(path(path_str))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(times)
        .mount(&mock.server)
        .await;
}

/// Mounts `PUT <path>` expecting `times` requests with exactly `{"ipv6_upstream": enabled}`.
async fn mount_upstream_put(mock: &MockEero, path_str: &str, enabled: bool, times: u64) {
    Mock::given(method("PUT"))
        .and(path(path_str))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        // `body_json` matches exactly, so a stray `ipv6_downstream` would never match.
        .and(body_json(json!({ "ipv6_upstream": enabled })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(times)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn set_ipv6_upstream_puts_only_the_upstream_flag() -> anyhow::Result<()> {
    for enabled in [true, false] {
        let mock = MockEero::start().await;
        mount_upstream_put(&mock, SETTINGS_PATH, enabled, 1).await;

        let client = client(&mock).await;
        let env = client
            .set_ipv6_upstream(enabled, Some("network-0001"), None)
            .await?;
        assert_eq!(env.into_value(), ok_envelope(), "enabled={enabled}");
    }
    Ok(())
}

#[tokio::test]
async fn set_ipv6_upstream_forwards_the_cached_settings_link_and_invalidates_only_the_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("dns_network_with_settings_link.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/eeros",
        fixture("eeros.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        1,
    )
    .await;
    mount_upstream_put(&mock, "/2.4/networks/network-0001/settings", true, 1).await;
    mount_upstream_put(&mock, SETTINGS_PATH, true, 0).await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    client
        .set_ipv6_upstream(true, Some("network-0001"), None)
        .await?;

    // `network[nid]` was dropped (second GET); eeros and devices were not (still one GET each).
    client.get_network(Some("network-0001"), false).await?;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_ipv6_upstream_prefers_the_callers_parent_over_the_cached_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("dns_network_with_settings_link.json"),
        1,
    )
    .await;
    // A third path, distinct from both the template and the cached `/2.4` link.
    mount_upstream_put(&mock, "/2.3/networks/network-0001/settings", false, 1).await;
    mount_upstream_put(&mock, "/2.4/networks/network-0001/settings", false, 0).await;
    mount_upstream_put(&mock, SETTINGS_PATH, false, 0).await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let caller_parent = json!({
        "meta": {"code": 200},
        "data": {"resources": {"settings": "/2.3/networks/network-0001/settings"}},
    });
    client
        .set_ipv6_upstream(false, Some("network-0001"), Some(&caller_parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_ipv6_upstream_without_a_network_is_missing_network_id_without_discovery()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, "/2.2/networks", fixture("networks.json"), 0).await;
    mount_upstream_put(&mock, SETTINGS_PATH, true, 0).await;

    let err = client(&mock)
        .await
        .set_ipv6_upstream(true, None, None)
        .await
        .expect_err("no explicit and no preferred network");
    assert!(matches!(err, Error::MissingNetworkId), "got {err:?}");

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_ipv6_upstream_failed_write_keeps_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        1,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path(SETTINGS_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let err = client
        .set_ipv6_upstream(true, Some("network-0001"), None)
        .await
        .expect_err("a 500 must surface");
    assert!(matches!(err, Error::Api { status: 500, .. }), "got {err:?}");

    // The GET mock allows exactly one request, so this is served from the cache.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}
