//! `Client` integration suite for `NetworksApi`'s pass-throughs, cache invalidation, and the
//! `/account` fallback of `get_networks`.
//!
//! **Judgment call (test-file split, 2026-09-25)**: the `/account`-fallback tests
//! (`get_networks_falls_back_to_account_when_the_list_is_empty`,
//! `get_networks_does_not_fall_back_when_the_list_is_non_empty`) exercise a quirk specific to
//! `Client::get_networks` (`src/client.rs`'s `NetworksApi`-facing accessor) — no other domain
//! shares this fallback — so they are treated as networks-domain tests and live here rather than
//! in `client_core.rs`. `account_and_networks_survive_a_write_that_invalidates_network_nid` and
//! `a_failed_write_does_not_invalidate_the_cache` are likewise placed here: both are driven by
//! `Client::set_network_name` (a `NetworksApi` write) proving its cache-invalidation contract,
//! even though the second also documents a `Client`-wide "failed write never invalidates" rule.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

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

// ===================== The `/account` fallback =====================

/// *** The `/account` fallback, end to end. ***
///
/// When `/networks` comes back empty, `Client::get_networks` forces a live `/account` fetch and,
/// if that yields a non-empty list, synthesises a new envelope: `meta` from the ORIGINAL
/// `/networks` response, `data.networks` from the account-derived list (brief gotcha G1). This
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
        .respond_with(ResponseTemplate::new(200).set_body_string(empty_networks.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_networks(false).await?;

    // The synthesised envelope's `meta` is the ORIGINAL `/networks` response's `meta`, not
    // `/account`'s (whose `server_time` is a different, fixture-owned value — see
    // `tests/fixtures/account.json`).
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

// ===================== One representative mutation =====================

#[tokio::test]
async fn set_network_name_reaches_the_settings_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
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

/// **Broken/restored in place** (see this suite's module docs and the task's red/green
/// requirement): the assertion below was temporarily changed to `.expect(1)` (asserting the
/// second `get_network` call is served from cache), run, and observed to fail — RED — because
/// `set_network_name` really does invalidate `network[nid]` and a second network request really
/// is made. It was then restored to `.expect(2)` — GREEN. See this task's final report for the
/// captured `cargo test` output of both runs.
#[tokio::test]
async fn set_network_name_invalidates_the_network_bucket() -> anyhow::Result<()> {
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
    client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== `account`/`networks` survive a network[nid] invalidation =====================

#[tokio::test]
async fn account_and_networks_survive_a_write_that_invalidates_network_nid() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
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
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
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

// ===================== Security review finding F5 =====================

/// **F5**: `sanitize_body_for_error` redacts a parseable JSON error body by *key* — a value that
/// looks like a credential but sits under an unrelated key (e.g. a guest Wi-Fi password echoed
/// back verbatim in a `400`'s `error` field) previously passed through untouched, because the
/// raw-text `looks_sensitive` fallback was only reachable for bodies that failed to parse as
/// JSON at all. This asserts the resulting `Error`'s `Display` never contains the submitted
/// password.
#[tokio::test]
async fn a_password_echoed_under_a_non_sensitive_key_is_suppressed_from_the_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"Invalid password: hunter2"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .set_guest_network(true, None, Some("hunter2"), Some("network-0001"))
        .await
        .unwrap_err();
    let rendered = err.to_string();
    assert!(!rendered.contains("hunter2"));
    Ok(())
}
