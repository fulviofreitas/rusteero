//! `Client` integration suite for the `AccountAPI` domain (new in v8.0.0), against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! Every write here is unverified against a live account (see `src/endpoints/account.rs`); these
//! tests pin the request shape and the `account`-bucket cache-invalidation contract, not live
//! behaviour. The read, `Client::get_account`, goes through `AccountApi::get_account`, and
//! `Client::get_networks` falls back to it when `/networks` lists nothing; the tests for both
//! are at the end of this file.

mod common;

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{body_string, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::cache::CacheKey;
use rusteero::client::Client;
use rusteero::error::Error;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

// ===================== set_account_name resets the account cache =====================

#[tokio::test]
async fn set_account_name_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/name"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_name("New Name").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_email is NOT invalidated =====================

#[tokio::test]
async fn set_account_email_does_not_invalidate_the_account_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/email"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_email("user@example.com").await?;
    // Second read is served from cache: only one GET to `/account` total.
    client.get_account(false).await?;
    Ok(())
}

// ===================== verify_account_email resets the account cache =====================

#[tokio::test]
async fn verify_account_email_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/email/verify"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.verify_account_email("123456").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_phone is NOT invalidated =====================

#[tokio::test]
async fn set_account_phone_does_not_invalidate_the_account_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/phone"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_phone("+15555550100").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== verify_account_phone resets the account cache =====================

#[tokio::test]
async fn verify_account_phone_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/phone/verify"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.verify_account_phone("654321").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_consents resets the account cache =====================

#[tokio::test]
async fn set_account_consents_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/consents"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_consents(true).await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== get_sms_countries: never cached, no network_id =====================

#[tokio::test]
async fn get_sms_countries_is_never_cached() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/countries/sms"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"countries":["US"]}})),
        )
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_sms_countries().await?;
    client.get_sms_countries().await?;
    Ok(())
}

// ===================== unit: CacheKey::Account round-trips through invalidate =====================

#[test]
fn cache_key_account_is_a_unit_variant() {
    // Compile-time/API-shape smoke test: `CacheKey::Account` must remain a bare unit variant
    // (no id parameter) for `set_account_name`/`verify_account_email`/`verify_account_phone`/
    // `set_account_consents` to keep invalidating it with `&CacheKey::Account`. A `const` (rather
    // than a `let` binding) keeps this a pure compile-time check with no runtime no-op binding.
    const _CHECK: CacheKey = CacheKey::Account;
}

// ===================== get_account goes through AccountApi =====================

fn account_body() -> Value {
    json!({"meta": {"code": 200}, "data": {"networks": {"data": []}}})
}

/// Mounts `GET /2.2/account` with both credentials required. Mounts answer in order: `once`
/// limits one to a single request.
async fn mount_account(mock: &MockEero, response: ResponseTemplate, times: u64, once: bool) {
    let mut mount = Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(response)
        .expect(times);
    if once {
        mount = mount.up_to_n_times(1);
    }
    mount.mount(&mock.server).await;
}

/// A `401 error.session.refresh` is refreshed once and replayed, and the replayed envelope is
/// what gets cached: the second `get_account` makes no request (the mock counts say so).
#[tokio::test]
async fn get_account_refreshes_on_a_session_refresh_signal_replays_and_caches() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_account(
        &mock,
        ResponseTemplate::new(401)
            .set_body_json(json!({"meta": {"code": 401, "error": "error.session.refresh"}})),
        1,
        true,
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    mount_account(
        &mock,
        ResponseTemplate::new(200).set_body_json(account_body()),
        1,
        false,
    )
    .await;

    let client = client(&mock).await;
    let first = client.get_account(false).await?;
    let second = client.get_account(false).await?;

    assert_eq!(first.as_value(), &account_body());
    assert_eq!(second.as_value(), &account_body());
    Ok(())
}

/// With no session the call fails before any request and nothing is cached, so a second call
/// fails the same way instead of being served an entry.
#[tokio::test]
async fn get_account_without_a_session_sends_nothing_and_caches_nothing() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_account(
        &mock,
        ResponseTemplate::new(200).set_body_json(account_body()),
        0,
        false,
    )
    .await;
    let client = Client::builder()
        .base_url(mock.uri())
        .session(None)
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    for _ in 0..2 {
        let err = client
            .get_account(false)
            .await
            .expect_err("there is no session");
        assert!(
            matches!(err, Error::Authentication { ref message, .. } if message == "Not authenticated"),
            "{err:?}"
        );
    }
    Ok(())
}

// ===================== get_networks: the /account fallback =====================

fn empty_networks_body() -> Value {
    json!({"meta": {"code": 200}, "data": {"networks": []}})
}

async fn mount_networks(mock: &MockEero, times: u64) {
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_networks_body()))
        .expect(times)
        .mount(&mock.server)
        .await;
}

fn synthesised_networks_body() -> Value {
    json!({"meta": {"code": 200}, "data": {"networks": [{"url": "/2.2/networks/77"}]}})
}

/// When `/account` itself fails, `get_networks` returns that error, caches nothing in either
/// bucket and selects no network, so every call retries.
#[tokio::test]
async fn get_networks_account_fallback_failure_propagates_and_caches_nothing() -> anyhow::Result<()>
{
    type Check = fn(&Error) -> bool;
    let cases: [(&str, u16, Check); 3] = [
        ("401", 401, |err| {
            matches!(err, Error::Authentication { .. })
        }),
        ("429", 429, |err| matches!(err, Error::RateLimit { .. })),
        ("500", 500, |err| {
            matches!(err, Error::Api { status: 500, .. })
        }),
    ];

    for (label, status, expected) in cases {
        let mock = MockEero::start().await;
        mount_networks(&mock, 2).await;
        mount_account(
            &mock,
            ResponseTemplate::new(status).set_body_json(json!({"meta": {"code": status}})),
            3,
            false,
        )
        .await;
        let client = client(&mock).await;

        for attempt in 1..=2 {
            let err = client
                .get_networks(false)
                .await
                .expect_err("the fallback failure propagates");
            assert!(expected(&err), "{label} attempt {attempt}: {err:?}");
        }
        // Nothing was cached in the account bucket either, so this goes to the network too.
        let err = client
            .get_account(false)
            .await
            .expect_err("the account read fails");
        assert!(expected(&err), "{label}: {err:?}");
        assert_eq!(client.preferred_network_id(), None, "{label}");
    }
    Ok(())
}

#[tokio::test]
async fn get_networks_account_fallback_timeout_propagates_and_caches_nothing() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_networks(&mock, 2).await;
    mount_account(
        &mock,
        ResponseTemplate::new(200)
            .set_body_json(account_body())
            .set_delay(Duration::from_millis(500)),
        3,
        false,
    )
    .await;
    // A short client-level timeout, so the held `/account` response times out.
    let client = Client::builder()
        .http_builder(reqwest::Client::builder().timeout(Duration::from_millis(100)))
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    for _ in 0..2 {
        let err = client
            .get_networks(false)
            .await
            .expect_err("the fallback times out");
        assert!(matches!(err, Error::Timeout { .. }), "{err:?}");
    }
    let err = client
        .get_account(false)
        .await
        .expect_err("the account read times out");
    assert!(matches!(err, Error::Timeout { .. }), "{err:?}");
    assert_eq!(client.preferred_network_id(), None);
    Ok(())
}

/// Nothing was cached by the failed attempt, so the next call reads both endpoints again and, if
/// `/account` now lists networks, returns the synthesised envelope and selects the first one.
#[tokio::test]
async fn get_networks_recovers_after_a_failed_account_fallback() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks(&mock, 2).await;
    mount_account(
        &mock,
        ResponseTemplate::new(429).set_body_json(json!({"meta": {"code": 429}})),
        1,
        true,
    )
    .await;
    mount_account(
        &mock,
        ResponseTemplate::new(200).set_body_json(
            json!({"meta": {"code": 200}, "data": {"networks": {"data": [{"url": "/2.2/networks/77"}]}}}),
        ),
        1,
        false,
    )
    .await;
    let client = client(&mock).await;

    let err = client
        .get_networks(false)
        .await
        .expect_err("the first fallback is rate limited");
    assert!(matches!(err, Error::RateLimit { .. }), "{err:?}");
    assert_eq!(client.preferred_network_id(), None);

    let recovered = client.get_networks(false).await?;

    assert_eq!(recovered.as_value(), &synthesised_networks_body());
    assert_eq!(client.preferred_network_id().as_deref(), Some("77"));
    Ok(())
}

/// Networks found on `/account`, bare or wrapped in `{"data": [...]}`, are returned in the
/// synthesised envelope, cached and selected.
#[tokio::test]
async fn get_networks_uses_account_networks_and_caches_them() -> anyhow::Result<()> {
    for account_data in [
        json!({"networks": [{"url": "/2.2/networks/77"}]}),
        json!({"networks": {"data": [{"url": "/2.2/networks/77"}]}}),
    ] {
        let mock = MockEero::start().await;
        mount_networks(&mock, 1).await;
        mount_account(
            &mock,
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta": {"code": 200}, "data": account_data})),
            1,
            false,
        )
        .await;
        let client = client(&mock).await;

        let first = client.get_networks(false).await?;
        let second = client.get_networks(false).await?;

        assert_eq!(
            first.as_value(),
            &synthesised_networks_body(),
            "{account_data}"
        );
        assert_eq!(
            second, first,
            "{account_data}: the second call is a cache hit"
        );
        assert_eq!(
            client.preferred_network_id().as_deref(),
            Some("77"),
            "{account_data}"
        );
    }
    Ok(())
}

/// Only a successful fallback that finds no networks gives a cacheable empty result: the
/// original `/networks` envelope, cached, with no network selected.
#[tokio::test]
async fn get_networks_caches_an_empty_successful_account_fallback() -> anyhow::Result<()> {
    let account_bodies = [
        json!({"meta": {"code": 200}, "data": {"networks": {"data": []}}}),
        json!({"meta": {"code": 200}, "data": {"networks": []}}),
        json!({"meta": {"code": 200}, "data": {}}),
        json!({"meta": {"code": 200}, "data": null}),
        json!({"meta": {"code": 200}, "data": {"networks": "unexpected"}}),
        json!([]),
    ];

    for account_body in account_bodies {
        let mock = MockEero::start().await;
        mount_networks(&mock, 1).await;
        mount_account(
            &mock,
            ResponseTemplate::new(200).set_body_json(&account_body),
            1,
            false,
        )
        .await;
        let client = client(&mock).await;

        let first = client.get_networks(false).await?;
        let second = client.get_networks(false).await?;

        assert_eq!(first.as_value(), &empty_networks_body(), "{account_body}");
        assert_eq!(second.as_value(), &empty_networks_body(), "{account_body}");
        assert_eq!(client.preferred_network_id(), None, "{account_body}");
    }
    Ok(())
}
