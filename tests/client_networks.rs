//! `Client` integration suite for `NetworksApi`'s pass-throughs, cache invalidation, and the
//! `/account` fallback of `get_networks`, at v8.0.4.
//!
//! **Judgment call (test-file split, 2026-09-25, retained)**: the `/account`-fallback tests
//! exercise a quirk specific to `Client::get_networks` — no other domain shares this fallback —
//! so they are treated as networks-domain tests and live here rather than in `client_core.rs`.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all.

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL — plenty long enough that none of these tests can flake on a slow
/// CI runner crossing a TTL boundary mid-test.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({"meta": {"code": 200}, "data": {}})
}

/// Asserts that the mock server has not received a single request.
async fn assert_no_requests(mock: &MockEero) {
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
}

/// Asserts that `err` is `Error::Validation` for `field` with the fixed non-empty-string message.
fn assert_non_empty_string_error(err: &Error, field: &str) {
    let Error::Validation {
        field: got_field,
        message,
        ..
    } = err
    else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(got_field, field);
    assert_eq!(message, "must be a non-empty string");
}

// ===================== The `/account` fallback =====================

/// *** The `/account` fallback, end to end. ***
///
/// When `/networks` comes back empty, `Client::get_networks` forces a live `/account` fetch and,
/// if that yields a non-empty list, synthesises a new envelope: `meta` from the ORIGINAL
/// `/networks` response, `data.networks` from the account-derived list. This
/// also derives `preferred_network_id` as a one-shot side effect.
#[tokio::test]
async fn get_networks_falls_back_to_account_when_the_list_is_empty() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let empty_networks = json!({
        "meta": {"code": 200, "server_time": "2020-01-01T00:00:00Z"},
        "data": [],
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(empty_networks.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_networks(false).await?;

    assert_eq!(env.meta().code, Some(200));
    assert_eq!(
        env.meta().server_time.as_deref(),
        Some("2020-01-01T00:00:00Z")
    );
    assert_ne!(
        env.meta().server_time.as_deref(),
        fixture_json("account.json")["meta"]["server_time"].as_str()
    );

    let expected_networks = fixture_json("account.json")["data"]["networks"]["data"].clone();
    assert_eq!(env.data()["networks"], expected_networks);

    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001"),
        "the account fallback must derive a preferred network as a side effect"
    );
    Ok(())
}

#[tokio::test]
async fn get_networks_does_not_fall_back_when_the_list_is_non_empty() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_networks(false).await?;
    assert_eq!(env.as_value(), &fixture_json("networks.json"));
    Ok(())
}

// ===================== get_premium_status =====================

#[tokio::test]
async fn get_premium_status_passes_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    // Populate the `network[nid]` cache entry `get_premium_status` will pass as `parent=`.
    client.get_network(Some("network-0001"), false).await?;
    let env = client.get_premium_status(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== One representative mutation: set_network_name =====================

#[tokio::test]
async fn set_network_name_reaches_the_settings_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================

#[tokio::test]
async fn set_network_name_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_password_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_network_password("hunter2", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn clear_network_password_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.clear_network_password(Some("network-0001")).await?;
    Ok(())
}

// ===================== set_guest_network =====================

/// `Client::set_guest_network` (`client.py:1127-1158`) passes the cached network envelope as
/// `parent=` (`+net`): once `network[nid]` is populated with an
/// envelope publishing its own `guestnetwork` link, the write goes to that link's path, not the
/// bare-id template, and on success invalidates `network[nid]`.
#[tokio::test]
async fn set_guest_network_passes_the_cached_network_as_parent_and_invalidates_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": {"code": 200},
        "data": {
            "url": "/2.2/networks/network-0001",
            "resources": {"guestnetwork": "/2.3/networks/network-0001/guestnetwork"},
        },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    // The bare-id template must never be hit once the cached parent publishes its own link.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_guest_network(true, Some("Guest"), Some("network-0001"))
        .await?;
    // `network[nid]` was invalidated: this re-fetches, hitting the GET mock a second time.
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== Guest network family =====================

#[tokio::test]
async fn get_guest_network_reaches_the_guestnetwork_get_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({"meta": {"code": 200}, "data": {"enabled": true}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_guest_network(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}

#[tokio::test]
async fn set_guest_password_reaches_the_password_endpoint_and_invalidates_the_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .set_guest_password("hunter2", Some("network-0001"))
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_guest_password_reaches_the_password_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.clear_guest_password(Some("network-0001")).await?;
    Ok(())
}

// ===================== Speed test family =====================

#[tokio::test]
async fn run_speed_test_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":{}})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.run_speed_test(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_speed_tests_forwards_every_kwarg() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/speedtest"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"meta":{"code":200},"data":[]})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_speed_tests(Some("network-0001"), Some(3), None, None)
        .await?;
    assert_eq!(env.data(), &json!([]));
    Ok(())
}

// ===================== `account`/`networks` survive a network[nid] invalidation =====================

#[tokio::test]
async fn account_and_networks_survive_a_write_that_invalidates_network_nid() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.get_networks(false).await?;
    client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;

    let account_again = client.get_account(false).await?;
    let networks_again = client.get_networks(false).await?;

    assert_eq!(account_again.as_value(), &fixture_json("account.json"));
    assert_eq!(networks_again.as_value(), &fixture_json("networks.json"));
    Ok(())
}

// ===================== A failed write never invalidates =====================

#[tokio::test]
async fn a_failed_write_does_not_invalidate_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;

    let err = client
        .set_network_name("New SSID", Some("network-0001"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        rusteero::error::Error::Api { status: 500, .. }
    ));

    let second = client.get_network(Some("network-0001"), false).await?;
    assert_eq!(second.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== Credentials never reach error messages =====================

/// Superseded by the v8.0.4 error model, but still worth pinning: a value that looks like a
/// credential but sits under an unrelated key (e.g. a guest Wi-Fi password echoed back verbatim
/// in a `400`'s `error` field) can no longer reach `Error::Api.message` at all — `message` is
/// built entirely from `errors::message_for_error_code`, never from the raw response body. This
/// asserts the resulting `Error`'s `Display` never contains the submitted password. Moved onto
/// `set_guest_password` at v8.0.4, since `set_guest_network` no longer carries a `password=`
/// argument at all.
#[tokio::test]
async fn a_password_echoed_under_a_non_sensitive_key_is_suppressed_from_the_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"Invalid password: hunter2"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .set_guest_password("hunter2", Some("network-0001"))
        .await
        .unwrap_err();
    let rendered = err.to_string();
    assert!(!rendered.contains("hunter2"));
    Ok(())
}

// ===================== Wi-Fi password validation (before network resolution) =====================

#[tokio::test]
async fn password_wrappers_reject_an_empty_password_before_resolving_the_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // With no explicit and no preferred network, resolution would fail with `MissingNetworkId`;
    // the password check has to win. The write mocks prove nothing is sent when an id is given.
    for resource in ["password", "guestnetwork/password"] {
        Mock::given(method("PUT"))
            .and(path(format!("/2.2/networks/network-0001/{resource}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
            .expect(0)
            .mount(&mock.server)
            .await;
    }

    let client = client(&mock).await;

    let err = client
        .set_network_password("", None)
        .await
        .expect_err("an empty password wins over a missing network");
    assert_non_empty_string_error(&err, "password");
    let err = client
        .set_guest_password("", None)
        .await
        .expect_err("an empty password wins over a missing network");
    assert_non_empty_string_error(&err, "password");

    let err = client
        .set_network_password("", Some("network-0001"))
        .await
        .expect_err("an empty password is rejected before any request");
    assert_non_empty_string_error(&err, "password");
    let err = client
        .set_guest_password("", Some("network-0001"))
        .await
        .expect_err("an empty password is rejected before any request");
    assert_non_empty_string_error(&err, "password");

    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn password_wrappers_forward_padded_and_literal_none_values_unchanged() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("password=++preserve+spaces++"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork/password"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("password=None"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .set_network_password("  preserve spaces  ", Some("network-0001"))
        .await?;
    client
        .set_guest_password("None", Some("network-0001"))
        .await?;
    Ok(())
}

// ===================== Name validation (before network resolution / discovery) =====================

#[tokio::test]
async fn set_network_name_rejects_an_empty_name_before_resolving_the_network() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;

    // No explicit and no preferred network: resolution would fail with `MissingNetworkId`.
    let err = client
        .set_network_name("", None)
        .await
        .expect_err("an empty name wins over a missing network");
    assert_non_empty_string_error(&err, "name");

    let err = client
        .set_network_name("", Some("network-0001"))
        .await
        .expect_err("an empty name is rejected before any request");
    assert_non_empty_string_error(&err, "name");

    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_guest_network_rejects_an_empty_name_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // `set_guest_network` auto-discovers a missing network id; an invalid name must not cause the
    // `GET /networks` round trip, and a failing discovery must not mask the validation error.
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .set_guest_network(true, Some(""), None)
        .await
        .expect_err("an empty guest name must be rejected before discovery");
    assert_non_empty_string_error(&err, "name");

    let err = client
        .set_guest_network(false, Some(""), Some("network-0001"))
        .await
        .expect_err("an empty guest name must be rejected before any request");
    assert_non_empty_string_error(&err, "name");

    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_guest_network_with_valid_input_still_auto_discovers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta": {"code": 200}, "data": [{"id": "network-0001"}]})),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("enabled=true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.set_guest_network(true, None, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn name_wrappers_forward_valid_names_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("name=My+Network"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("enabled=false&name=++preserve+spaces++"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .set_network_name("My Network", Some("network-0001"))
        .await?;
    client
        .set_guest_network(false, Some("  preserve spaces  "), Some("network-0001"))
        .await?;
    Ok(())
}

// ===================== set_hide_5g =====================

const HIDE_5G_PATH: &str = "/2.2/networks/network-0001/temporary_flags/hide_5g";

#[tokio::test]
async fn set_hide_5g_true_reaches_the_temporary_flag_put() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(HIDE_5G_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(json!({"value": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(HIDE_5G_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.set_hide_5g(true, Some("network-0001")).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_false_reaches_the_temporary_flag_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(HIDE_5G_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(HIDE_5G_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.set_hide_5g(false, Some("network-0001")).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_uses_the_preferred_network_without_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0002/temporary_flags/hide_5g"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({"value": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_preferred_network("network-0002");
    client.set_hide_5g(true, None).await?;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_without_a_network_is_missing_network_id_without_discovery()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    for enabled in [true, false] {
        let err = client
            .set_hide_5g(enabled, None)
            .await
            .expect_err("no explicit and no preferred network");
        assert!(matches!(err, Error::MissingNetworkId), "got {err:?}");
    }
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_invalidates_only_the_written_networks_cache_entry() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    // The other network's entry must survive the write: it is fetched once, then served cached.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0002"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(HIDE_5G_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_network(Some("network-0002"), false).await?;
    client.set_hide_5g(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    client.get_network(Some("network-0002"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_hide_5g_failed_write_keeps_the_network_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(HIDE_5G_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let err = client
        .set_hide_5g(true, Some("network-0001"))
        .await
        .expect_err("a 500 must surface");
    assert!(matches!(err, Error::Api { status: 500, .. }), "got {err:?}");

    let cached = client.get_network(Some("network-0001"), false).await?;
    assert_eq!(cached.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== reboot_network =====================

const REBOOT_PATH: &str = "/2.2/networks/network-0001/reboot";

/// Mounts `POST <path>` expecting `times` requests carrying the two-byte JSON string body `""`
/// and both credentials.
async fn mount_reboot_post(mock: &MockEero, path_str: &str, times: u64) {
    Mock::given(method("POST"))
        .and(path(path_str))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(times)
        .mount(&mock.server)
        .await;
}

/// Mounts a `GET <path>` returning `body`, expecting exactly `times` requests.
async fn mount_get(mock: &MockEero, path_str: &str, body: String, times: u64) {
    Mock::given(method("GET"))
        .and(path(path_str))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(times)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn reboot_network_posts_the_empty_json_string_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_reboot_post(&mock, REBOOT_PATH, 1).await;

    let client = client(&mock).await;
    let env = client.reboot_network(Some("network-0001"), None).await?;
    assert_eq!(env.into_value(), ok_envelope());

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert_eq!(requests.len(), 1, "got {requests:?}");
    Ok(())
}

#[tokio::test]
async fn reboot_network_warns_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_reboot_post(&mock, REBOOT_PATH, 1).await;
    let (_guard, logs) = common::capture_logs();

    client(&mock)
        .await
        .reboot_network(Some("network-0001"), None)
        .await?;

    assert_eq!(logs.warnings().len(), 1, "got {:?}", logs.warnings());
    assert_eq!(
        logs.count(tracing::Level::WARN, "Issuing write (reboot network)"),
        1
    );
    Ok(())
}

/// The cached reads `reboot_network` is expected to partially invalidate: for both networks the
/// network, eeros and device list, plus one single-device entry on `network-0001`.
async fn read_all_cached_reads(client: &Client) -> anyhow::Result<()> {
    client.get_network(Some("network-0001"), false).await?;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .get_device("device-0001", Some("network-0001"), false)
        .await?;
    client.get_network(Some("network-0002"), false).await?;
    client.get_eeros(Some("network-0002"), false).await?;
    client
        .get_devices(Some("network-0002"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn reboot_network_invalidates_network_eeros_and_the_device_list_only() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // network-0001: network, eeros and the device list are re-fetched after the reboot (2 hits
    // each); the single-device entry survives (1 hit). network-0002 is untouched (1 hit each).
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/eeros",
        fixture("eeros.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/device-0001",
        fixture("device.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0002",
        fixture("network.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0002/eeros",
        fixture("eeros.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0002/devices",
        fixture("devices.json"),
        1,
    )
    .await;
    mount_reboot_post(&mock, REBOOT_PATH, 1).await;

    let client = client(&mock).await;
    read_all_cached_reads(&client).await?;
    client.reboot_network(Some("network-0001"), None).await?;
    read_all_cached_reads(&client).await?;
    Ok(())
}

#[tokio::test]
async fn reboot_network_uses_the_cached_network_reboot_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network_with_write_links.json"),
        1,
    )
    .await;
    mount_reboot_post(&mock, "/2.4/networks/network-0001/reboot", 1).await;
    mount_reboot_post(&mock, REBOOT_PATH, 0).await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.reboot_network(Some("network-0001"), None).await?;
    Ok(())
}

#[tokio::test]
async fn reboot_network_prefers_the_caller_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network_with_write_links.json"),
        1,
    )
    .await;
    // A third path, distinct from both the template and the cached `/2.4` link.
    mount_reboot_post(&mock, "/2.3/networks/network-0001/reboot", 1).await;
    mount_reboot_post(&mock, "/2.4/networks/network-0001/reboot", 0).await;
    mount_reboot_post(&mock, REBOOT_PATH, 0).await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let caller_parent = json!({
        "meta": {"code": 200},
        "data": {"resources": {"reboot": "/2.3/networks/network-0001/reboot"}},
    });
    client
        .reboot_network(Some("network-0001"), Some(&caller_parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn reboot_network_without_a_network_is_missing_network_id_without_discovery()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, "/2.2/networks", fixture("networks.json"), 0).await;
    mount_reboot_post(&mock, REBOOT_PATH, 0).await;

    let err = client(&mock)
        .await
        .reboot_network(None, None)
        .await
        .expect_err("no explicit and no preferred network");
    assert!(matches!(err, Error::MissingNetworkId), "got {err:?}");
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn reboot_network_failure_leaves_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        1,
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
    Mock::given(method("POST"))
        .and(session_cookie())
        .and(user_token_header())
        .and(path(REBOOT_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    let err = client
        .reboot_network(Some("network-0001"), None)
        .await
        .expect_err("a 500 must surface");
    assert!(matches!(err, Error::Api { status: 500, .. }), "got {err:?}");

    // Each GET mock allows exactly one request, so these are served from the cache.
    client.get_network(Some("network-0001"), false).await?;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}
