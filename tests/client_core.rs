//! `Client` integration suite: core mechanics that are not specific to any one domain — the
//! cache actually working end to end, network-id resolution, and every place `Client` clears its
//! cache.
//!
//! Covers, against a local `wiremock` server per the crate's testing conventions: the cached
//! getters serving a second call from cache; `refresh_cache`/`cache_ttl(Duration::ZERO)`
//! bypassing that cache; TTL expiry (via `tokio::time::pause()`/`advance()`, never a real
//! `sleep`); the falsy-value rule end to end (`src/cache.rs`'s `is_falsy` — a real Python quirk,
//! not a bug); two networks' cache entries never colliding; network-id resolution
//! (`Client::ensure_network_id` — explicit id, preferred id, auto-discovery in both its `id` and
//! `url`-tail shapes, and the `Error::MissingNetworkId` failure path); every place `Client`
//! clears its cache (`clear_cache`, `logout`, `set_session_token`, `clear_session_token`); and
//! that a failed `logout` still clears the cache.
//!
//! `get_devices` is used throughout purely as a convenient, already-cached vehicle to exercise
//! these generic `Client`/`Cache` mechanics — none of the assertions here are about `DevicesApi`
//! business logic itself (that lives in `endpoints_devices.rs`/`client_devices.rs`). The
//! `/account` fallback of `get_networks`, and `get_networks`/`get_account`-specific caching, are
//! judged to be networks-domain tests and live in `client_networks.rs` instead — see that file's
//! module docs for the reasoning.
//!
//! Every caching test asserts on the wiremock `.expect(n)` call count, not just the returned value
//! — a caching test that only checks the envelope is not testing caching at all.
//! `logout_...`/`clear_session_token_...` restore a session directly at the `EeroApi` layer via
//! [`Client::api`] rather than through `Client::set_session_token` — deliberately, so that
//! restoring the ability to make a second request never itself clears the cache and masks a
//! regression in the very call site under test.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use reqwest::header::COOKIE;
use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, Request, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::consts::LEGACY_USER_AGENT;
use rusteero::error::{Error, StorageError};
use rusteero::storage::{CredentialStore, MemoryStore};
use rusteero::transport::StorageFailures;

/// Matches a request that carries no `Cookie` header at all — the same shape
/// `tests/transport.rs`'s own `no_cookie_header` uses, duplicated here rather than shared: this
/// file exercises `ClientBuilder::send_legacy_cookie`, `tests/transport.rs` exercises
/// `TransportBuilder::send_legacy_cookie` directly, and neither integration-test binary can import
/// from the other.
fn no_cookie_header(request: &Request) -> bool {
    !request.headers.contains_key(COOKIE)
}

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL. See [`client_with_ttl`] for a caller that needs a different TTL.
async fn client(mock: &MockEero) -> Client {
    client_with_ttl(mock, Duration::from_secs(60)).await
}

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with an explicit
/// cache TTL — used by the TTL-expiry and zero-TTL tests below.
async fn client_with_ttl(mock: &MockEero, cache_ttl: Duration) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .cache_ttl(cache_ttl)
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

// ===================== Caching =====================

#[tokio::test]
async fn get_devices_second_call_is_served_from_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let first = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    let second = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    assert_eq!(first.as_value(), &fixture_json("devices.json"));
    assert_eq!(second.as_value(), &fixture_json("devices.json"));
    Ok(())
}

#[tokio::test]
async fn get_devices_with_refresh_cache_true_always_hits_the_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), true, None, None)
        .await?;
    client
        .get_devices(Some("network-0001"), true, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_refetches_after_the_ttl_expires() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    // A `reqwest::ClientBuilder` with neither of the crate's default request/read timeouts
    // configured (see `Client::builder().http_builder(..)`'s own docs for this escape hatch).
    // Those timeouts are implemented as `tokio::time` sleeps racing the real response;
    // `tokio::time::pause()`'s documented auto-advance behaviour ("if the runtime has no work to
    // do, the clock is auto-advanced to the next pending timer") can fire one of those sleeps the
    // instant the `advance()` call below leaves the runtime transiently idle mid-request —
    // observed in practice as a flaky, spurious `Error::Timeout` (or an outright dropped
    // connection) on the second call. Dropping both timeouts removes that race entirely; the
    // real, unmocked socket I/O this test still performs is unaffected by the paused clock
    // either way. The redirect policy is forced to `Policy::none()` by `build()` regardless, so
    // there is no need to set one here.
    let http = reqwest::Client::builder();
    let client = Client::builder()
        .http_builder(http)
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .cache_ttl(Duration::from_secs(30))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;

    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    // The refetched response replaced the expired entry, so a third call is a cache hit: the
    // mock's `.expect(2)` would fail if it were fetched again.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_devices_with_zero_ttl_never_serves_from_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client_with_ttl(&mock, Duration::ZERO).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

/// *** The falsy-value rule end to end. ***
///
/// A `204 No Content` becomes `Envelope::empty()` (a literal `{}`), which is "falsy" under
/// `Cache::get`'s rule (`src/cache.rs`'s `is_falsy` — checked against the *whole* wire envelope,
/// not just its `data` field, so `{"meta": ..., "data": {}}` would NOT qualify: only a
/// genuinely empty top-level object does). This is a faithful port of a real Python quirk
/// (`client.py`'s `if cached:` truthiness guard) — a second call within the TTL must still
/// re-hit the network, so `.expect(2)` here is the correct assertion, not `.expect(1)`.
#[tokio::test]
async fn falsy_cached_value_is_never_served_from_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(204))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let first = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    let second = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    assert_eq!(first.as_value(), &json!({}));
    assert_eq!(second.as_value(), &json!({}));
    Ok(())
}

#[tokio::test]
// `network_a_devices`/`network_b_devices` intentionally mirror each other (same shape, only the
// network letter differs) — that pairing is the point of the test, not an accident worth
// renaming around.
#[allow(clippy::similar_names)]
async fn devices_for_different_networks_do_not_collide() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_a_devices = json!({"meta": {"code": 200}, "data": [{"mac": "AA:BB:CC:00:00:0A"}]});
    let network_b_devices = json!({"meta": {"code": 200}, "data": [{"mac": "AA:BB:CC:00:00:0B"}]});

    Mock::given(method("GET"))
        .and(path("/2.2/networks/net-a/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_a_devices.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/net-b/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_b_devices.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let a_first = client.get_devices(Some("net-a"), false, None, None).await?;
    let b_first = client.get_devices(Some("net-b"), false, None, None).await?;
    let a_second = client.get_devices(Some("net-a"), false, None, None).await?;
    let b_second = client.get_devices(Some("net-b"), false, None, None).await?;

    assert_eq!(a_first.as_value(), &network_a_devices);
    assert_eq!(b_first.as_value(), &network_b_devices);
    assert_eq!(
        a_first, a_second,
        "net-a's second call must be served from its own cache entry"
    );
    assert_eq!(
        b_first, b_second,
        "net-b's second call must be served from its own cache entry"
    );
    assert_ne!(
        a_first.as_value(),
        b_first.as_value(),
        "the two networks' cached entries must never collide"
    );
    Ok(())
}

// ===================== Network-id resolution =====================

#[tokio::test]
async fn explicit_network_id_wins_over_the_preferred_one() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/explicit-net/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    // Never hit if resolution incorrectly preferred the preferred-network state instead of the
    // explicit argument.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/preferred-net/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_preferred_network("preferred-net");

    let env = client
        .get_devices(Some("explicit-net"), false, None, None)
        .await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

#[tokio::test]
async fn preferred_network_is_used_when_no_explicit_id_is_given() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/preferred-net/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.set_preferred_network("preferred-net");

    let env = client.get_devices(None, false, None, None).await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

/// Auto-discovery shape 1: the first `/networks` entry carries a bare, non-empty `id` field.
#[tokio::test]
async fn auto_discovery_uses_the_first_networks_bare_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let networks_body = json!({"meta": {"code": 200}, "data": [{"id": "auto-net-a"}]});

    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(networks_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/auto-net-a/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_devices(None, false, None, None).await?;

    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    assert_eq!(client.preferred_network_id().as_deref(), Some("auto-net-a"));
    Ok(())
}

/// Auto-discovery shape 2: the first `/networks` entry has no `id` field at all, so the network
/// id is derived from the trailing path segment of its `url` via `id_from_url`.
#[tokio::test]
async fn auto_discovery_falls_back_to_the_url_tail_when_id_is_absent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let networks_body =
        json!({"meta": {"code": 200}, "data": [{"url": "/2.2/networks/auto-net-b"}]});

    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(networks_body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/auto-net-b/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_devices(None, false, None, None).await?;

    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    assert_eq!(client.preferred_network_id().as_deref(), Some("auto-net-b"));
    Ok(())
}

#[tokio::test]
async fn missing_network_id_after_failed_auto_discovery_makes_no_downstream_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let empty_networks = json!({"meta": {"code": 200}, "data": []});
    // The `/networks` list is empty, so `Client::get_networks` will itself attempt the
    // `/account` fallback (covered in `client_networks.rs`) before auto-discovery gives up; that
    // fallback must also come back empty here so resolution genuinely finds nothing.
    let empty_account = json!({"meta": {"code": 200}, "data": {"networks": {"data": []}}});

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
        .respond_with(ResponseTemplate::new(200).set_body_string(empty_account.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .get_devices(None, false, None, None)
        .await
        .expect_err("no network id can be resolved");
    assert!(matches!(err, Error::MissingNetworkId));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(
        requests.iter().all(|r| !r.url.path().contains("/devices")),
        "no device request should ever have been attempted: {requests:?}"
    );
    Ok(())
}

// ===================== Session and cache invalidation =====================

#[tokio::test]
async fn clear_cache_forces_the_next_call_to_re_hit_the_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client.clear_cache();
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn logout_clears_the_cache_so_a_subsequent_get_re_hits_the_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    client.logout().await?;

    // `logout` also clears the in-memory session, so a session must be restored before this
    // cached getter can be called again. Restore it directly at the `EeroApi` layer (the escape
    // hatch `Client::api` documents) rather than via `Client::set_session_token`, which
    // deliberately clears the cache itself (covered separately below) — if that shortcut were
    // used here instead, this test could pass even if `Client::logout` never cleared the cache
    // at all.
    client.api().auth().set_session_token(TEST_TOKEN).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_session_token_clears_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    client.set_session_token(TEST_TOKEN).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_session_token_clears_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    client.clear_session_token().await?;
    // `clear_session_token` nulls the in-memory session token itself, so — exactly as in the
    // `logout` test above — restore it via the `EeroApi` layer directly, never through
    // `Client::set_session_token`, so this call cannot be the one masking a missing cache clear
    // in `clear_session_token` itself.
    client.api().auth().set_session_token(TEST_TOKEN).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

/// v8.0.4 logout contract: `AuthApi::logout` clears the in-memory
/// session and credential store unconditionally, on every outcome, and — at `v8.0.4` — never
/// propagates a network/API error from the `logout` request itself (`api/auth.py:315-327`); see
/// `src/auth/mod.rs`'s own docs. This primes the `network` bucket, forces the `logout` request to
/// fail on the wire (500), and asserts (a) `Client::logout` still reports success (`Ok(true)`)
/// rather than surfacing the 500, and (b) the cache can no longer serve the second `get_network`
/// call: with both the cache and the session gone, it must fail with `Error::Authentication`, not
/// silently return the stale envelope.
#[tokio::test]
async fn logout_failure_is_swallowed_but_still_clears_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;

    let logged_out = client
        .logout()
        .await
        .expect("v8.0.4 logout never propagates a network/API error");
    assert!(logged_out);
    assert!(!client.is_authenticated());

    // Before the fix this served the pre-logout envelope straight from cache (`Ok`), with no
    // auth check at all. After the fix the cache is empty and the session is gone, so this must
    // fail closed rather than leak the earlier authenticated response.
    let after = client.get_network(Some("network-0001"), false).await;
    assert!(matches!(after, Err(Error::Authentication { .. })));
    Ok(())
}

// ===================== ClientBuilder options (v8.0.4: send_legacy_cookie/accept_language/get_retries) =====================

/// `ClientBuilder::send_legacy_cookie(false)` must reach the underlying `Transport`: the primary
/// `X-User-Token` header is still sent, but the legacy `Cookie: s=<token>` header is omitted
/// entirely. Both matchers are attached to the *same* `Mock`, so a request that still carried a
/// `Cookie` header (the builder option silently not forwarded) would fail to match at all — the
/// request would then get wiremock's default 404, `client.get_devices` would return an `Err`
/// instead of the expected envelope, and the `.expect(1)` guard would additionally report the
/// mock as never hit.
#[tokio::test]
async fn send_legacy_cookie_false_omits_the_cookie_header_but_keeps_the_user_token_header()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(user_token_header())
        .and(no_cookie_header)
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .send_legacy_cookie(false)
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    let env = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

/// The default (`ClientBuilder::send_legacy_cookie` never called) still sends both credentials —
/// the sibling case to the test above, pinning that the new builder option is opt-out, not a
/// behavioural change to `Client::builder()`'s existing default.
#[tokio::test]
async fn send_legacy_cookie_defaults_to_true_when_never_called() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

/// `ClientBuilder::accept_language` must reach the underlying `Transport`'s
/// `X-Accept-Language` header.
#[tokio::test]
async fn accept_language_is_forwarded_to_the_transport() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(wiremock::matchers::header("x-accept-language", "fr-FR"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .accept_language("fr-FR")
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    let env = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

// ===================== Session persistence through the Client =====================

/// The longest a parked store call waits for [`GateStore::open`]. A test whose assertions fail
/// between `wait_parked()` and `open()` would otherwise leave the blocking-pool thread parked
/// forever, and the runtime's drop would wait for it without end.
const PARK_LIMIT: Duration = Duration::from_secs(10);

/// Which `GateStore` operation parks until the gate opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parks {
    Nothing,
    Saves,
    Loads,
    Clears,
}

/// A [`CredentialStore`] test double over a [`MemoryStore`] whose `save`, `load` or `clear` can be
/// parked on a blocking-pool thread until [`GateStore::open`], and whose `save` and `clear` can be
/// made to fail. Trimmed copy of the one in `tests/auth.rs`: integration-test binaries cannot
/// share code except through `tests/common`.
///
/// A parked call never outlives [`PARK_LIMIT`] (it then fails with a [`StorageError`]), and one
/// that arrives on the thread that built the store, which is the thread driving a
/// `#[tokio::test]` runtime, panics instead of parking: a store call that ran inline on the
/// runtime would otherwise block the very runtime whose timeouts are meant to report it.
#[derive(Debug)]
struct GateStore {
    inner: MemoryStore,
    parks: Parks,
    test_thread: ThreadId,
    load_threads: Mutex<Vec<ThreadId>>,
    gate_open: Mutex<bool>,
    gate_changed: Condvar,
    parked: AtomicBool,
    failing: AtomicBool,
}

impl GateStore {
    fn make(parks: Parks, failing: bool) -> Self {
        Self {
            inner: MemoryStore::new(),
            parks,
            test_thread: std::thread::current().id(),
            load_threads: Mutex::new(Vec::new()),
            gate_open: Mutex::new(false),
            gate_changed: Condvar::new(),
            parked: AtomicBool::new(false),
            failing: AtomicBool::new(failing),
        }
    }

    /// A store that parks every `save` until opened.
    fn parking_saves() -> Arc<Self> {
        Arc::new(Self::make(Parks::Saves, false))
    }

    /// A store whose `save` and `clear` fail at once, without parking.
    fn failing() -> Arc<Self> {
        Arc::new(Self::make(Parks::Nothing, true))
    }

    /// A store that parks every `clear` until opened.
    fn parking_clears() -> Arc<Self> {
        Arc::new(Self::make(Parks::Clears, false))
    }

    /// A store that already holds `token` and parks every `load` until opened.
    fn parking_loads(token: &str) -> Arc<Self> {
        let store = Self::make(Parks::Loads, false);
        store
            .inner
            .save(&Session::from_token(token))
            .expect("seeding the in-memory record cannot fail");
        Arc::new(store)
    }

    /// Parks the calling blocking-pool thread until [`GateStore::open`], or fails after
    /// [`PARK_LIMIT`].
    fn park(&self) -> Result<(), StorageError> {
        assert_ne!(
            std::thread::current().id(),
            self.test_thread,
            "a store call ran on the runtime thread instead of the blocking pool"
        );
        self.parked.store(true, Ordering::SeqCst);
        let open = self.gate_open.lock().expect("gate lock");
        let (_open, waited) = self
            .gate_changed
            .wait_timeout_while(open, PARK_LIMIT, |open| !*open)
            .expect("gate wait");
        if waited.timed_out() {
            return Err(StorageError::Backend {
                backend: "gate".to_owned(),
                message: "the gate was never opened".to_owned(),
            });
        }
        Ok(())
    }

    fn open(&self) {
        *self.gate_open.lock().expect("gate lock") = true;
        self.gate_changed.notify_all();
    }

    /// Opens the gate when dropped, so a test that fails with a store call still parked releases
    /// the blocking-pool thread instead of hanging the runtime's shutdown.
    fn release_on_drop(self: &Arc<Self>) -> OpenOnDrop {
        OpenOnDrop(Arc::clone(self))
    }

    async fn wait_parked(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.parked.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("a store call should have reached the parking gate");
    }

    fn failure(&self) -> Result<(), StorageError> {
        if self.failing.load(Ordering::SeqCst) {
            Err(StorageError::Backend {
                backend: "gate".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

/// See [`GateStore::release_on_drop`].
struct OpenOnDrop(Arc<GateStore>);

impl Drop for OpenOnDrop {
    fn drop(&mut self) {
        self.0.open();
    }
}

impl CredentialStore for GateStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.load_threads
            .lock()
            .expect("load threads lock")
            .push(std::thread::current().id());
        if self.parks == Parks::Loads {
            self.park()?;
        }
        self.inner.load()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        if self.parks == Parks::Saves {
            self.park()?;
        }
        self.failure()?;
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StorageError> {
        if self.parks == Parks::Clears {
            self.park()?;
        }
        self.failure()?;
        self.inner.clear()
    }
}

async fn client_with_store(
    mock: &MockEero,
    store: &Arc<GateStore>,
    policy: StorageFailures,
) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .store(Some(Arc::clone(store) as Arc<dyn CredentialStore>))
        .storage_failures(policy)
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

async fn mount_devices(mock: &MockEero, times: u64) {
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(times)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn client_set_session_token_offloads_the_store_write_and_resets_the_cache_before_it()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_devices(&mock, 2).await;
    let store = GateStore::parking_saves();
    let _release = store.release_on_drop();
    let client = Arc::new(client_with_store(&mock, &store, StorageFailures::Warn).await);

    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        mock.server.received_requests().await.map(|r| r.len()),
        Some(1)
    );

    let task = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.set_session_token(TEST_TOKEN).await }
    });
    store.wait_parked().await;

    // The heartbeat ticks while the store write is parked, so the runtime is not blocked.
    tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the runtime keeps making progress while the store write is parked");
    assert!(
        !task.is_finished(),
        "the setter waits for the parked store write"
    );

    // The cache was reset in the same step that installed the session, before the store hop, so
    // a read made while the store write is still parked goes to the network: no task sees the new
    // session next to the previous session's cache.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        mock.server.received_requests().await.map(|r| r.len()),
        Some(2)
    );

    store.open();
    task.await??;

    // Completing the store write resets nothing a second time: the entry filled during the hop,
    // under the new session, is still served from the cache.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        mock.server.received_requests().await.map(|r| r.len()),
        Some(2)
    );
    Ok(())
}

/// The caller drops the setter's future while the store write is parked (a timeout around a slow
/// keyring, a `select!` branch, an aborted task). The new session is already installed and the
/// store write still runs to completion, so the facade state derived from the previous session
/// must be gone too: a reset that waited for the store would never run.
#[tokio::test]
async fn dropping_set_session_token_during_the_store_hop_still_resets_the_facade()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;
    mount_devices_of(&mock, "network-0001", 2).await;
    let store = GateStore::parking_saves();
    let _release = store.release_on_drop();
    let client = Arc::new(client_with_store(&mock, &store, StorageFailures::Warn).await);

    client.get_networks(false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );

    let task = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.set_session_token(TEST_TOKEN).await }
    });
    store.wait_parked().await;
    task.abort();
    assert!(
        task.await.expect_err("the task was aborted").is_cancelled(),
        "the setter's future was dropped while the store write was parked"
    );

    assert_eq!(
        client.preferred_network_id(),
        None,
        "the discovered network belonged to the previous session"
    );
    // The cache was reset too: the same read goes back to the network, which the second
    // expectation on the devices mock counts.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

/// The session-clearing calls, for the tests that park the credential-store clear.
#[derive(Debug, Clone, Copy)]
enum Clearing {
    ClearSessionToken,
    ClearAuthData,
    Logout,
}

impl Clearing {
    const ALL: [Self; 3] = [Self::ClearSessionToken, Self::ClearAuthData, Self::Logout];

    /// Mounts what the call itself needs on the wire.
    async fn mount(self, mock: &MockEero) {
        if matches!(self, Self::Logout) {
            Mock::given(method("POST"))
                .and(path("/2.2/logout"))
                .and(user_token_header())
                .and(session_cookie())
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&mock.server)
                .await;
        }
    }

    async fn run(self, client: &Client) -> Result<(), Error> {
        match self {
            Self::ClearSessionToken => client.clear_session_token().await,
            Self::ClearAuthData => client.clear_auth_data().await,
            Self::Logout => client.logout().await.map(|_| ()),
        }
    }
}

/// Mounts what a [`Clearing`] test needs and fills the client's cache and discovered network.
async fn client_holding_the_previous_sessions_state(
    mock: &MockEero,
    clearing: Clearing,
    store: &Arc<GateStore>,
) -> anyhow::Result<Client> {
    clearing.mount(mock).await;
    mount_networks_once(mock, json!([{"id": "network-0001"}])).await;
    mount_devices_of(mock, "network-0001", 1).await;
    let client = client_with_store(mock, store, StorageFailures::Warn).await;
    client.get_networks(false).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );
    Ok(client)
}

/// Clearing the session resets the cache and the discovered network in the same step as the
/// in-memory session, before the parked store clear: a read made meanwhile finds neither a
/// session nor the previous session's cached entry.
#[tokio::test]
async fn clearing_the_session_resets_the_facade_before_the_store_hop() -> anyhow::Result<()> {
    for clearing in Clearing::ALL {
        let mock = MockEero::start().await;
        let store = GateStore::parking_clears();
        let _release = store.release_on_drop();
        let client =
            Arc::new(client_holding_the_previous_sessions_state(&mock, clearing, &store).await?);

        let task = tokio::spawn({
            let client = Arc::clone(&client);
            async move { clearing.run(&client).await }
        });
        store.wait_parked().await;
        assert!(
            !task.is_finished(),
            "{clearing:?} waits for the parked store clear"
        );

        assert!(!client.is_authenticated(), "{clearing:?}");
        let err = client
            .get_devices(Some("network-0001"), false, None, None)
            .await
            .expect_err("the previous session's cache must not answer for a cleared session");
        assert!(
            matches!(err, Error::Authentication { .. }),
            "{clearing:?}: {err:?}"
        );
        assert_eq!(client.preferred_network_id(), None, "{clearing:?}");

        store.open();
        task.await??;
    }
    Ok(())
}

/// The caller drops the clearing call's future while the store clear is parked. The session is
/// already gone and the store clear still runs to completion, so the cache and the discovered
/// network must be gone as well.
#[tokio::test]
async fn dropping_a_clearing_call_during_the_store_hop_still_resets_the_facade()
-> anyhow::Result<()> {
    for clearing in Clearing::ALL {
        let mock = MockEero::start().await;
        let store = GateStore::parking_clears();
        let _release = store.release_on_drop();
        let client =
            Arc::new(client_holding_the_previous_sessions_state(&mock, clearing, &store).await?);

        let task = tokio::spawn({
            let client = Arc::clone(&client);
            async move { clearing.run(&client).await }
        });
        store.wait_parked().await;
        task.abort();
        assert!(
            task.await.expect_err("the task was aborted").is_cancelled(),
            "{clearing:?}: the future was dropped while the store clear was parked"
        );

        assert!(!client.is_authenticated(), "{clearing:?}");
        let err = client
            .get_devices(Some("network-0001"), false, None, None)
            .await
            .expect_err("the previous session's cache must not answer for a cleared session");
        assert!(
            matches!(err, Error::Authentication { .. }),
            "{clearing:?}: {err:?}"
        );
        assert_eq!(client.preferred_network_id(), None, "{clearing:?}");
    }
    Ok(())
}

#[tokio::test]
async fn client_set_session_token_failing_under_fatal_still_resets_the_cache() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_devices(&mock, 2).await;
    let store = GateStore::failing();
    let client = client_with_store(&mock, &store, StorageFailures::Fatal).await;

    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;

    let err = client
        .set_session_token(TEST_TOKEN)
        .await
        .expect_err("Fatal policy surfaces the failed save");
    assert!(matches!(err, Error::Storage(_)), "got {err:?}");
    assert_eq!(
        client
            .session()
            .expect("installed before persisting")
            .token()
            .expose_secret(),
        TEST_TOKEN,
        "the in-memory session changed even though persisting failed"
    );

    // The new session is already in place when the error is returned, so nothing cached under
    // the previous one may be served to it: the same read goes to the network again.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn client_credentials_persisted_delegates_through_the_facades() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store = GateStore::failing();
    let client = client_with_store(&mock, &store, StorageFailures::Warn).await;

    assert!(client.credentials_persisted());
    assert!(client.api().credentials_persisted());
    assert!(client.api().auth().credentials_persisted());

    client.set_session_token("replacement-token").await?;
    assert!(
        client.is_authenticated(),
        "Warn keeps the session usable in memory"
    );
    assert!(!client.credentials_persisted());
    assert!(!client.api().credentials_persisted());
    assert!(!client.api().auth().credentials_persisted());

    client.clear_session_token().await?;
    assert!(client.credentials_persisted());
    assert!(client.api().credentials_persisted());
    assert!(client.api().auth().credentials_persisted());
    Ok(())
}

#[tokio::test]
async fn client_without_a_store_reports_credentials_persisted() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;

    assert!(client.credentials_persisted());
    client.set_session_token("replacement-token").await?;
    assert!(client.credentials_persisted());
    Ok(())
}

#[tokio::test]
async fn client_build_initial_store_load_does_not_block_the_runtime() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store = GateStore::parking_loads(TEST_TOKEN);
    let _release = store.release_on_drop();

    // No `.session(..)`, so `build` loads the stored session; that load is parked.
    let builder = Client::builder()
        .base_url(mock.uri())
        .store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>));
    let build = tokio::spawn(builder.build());
    store.wait_parked().await;

    tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the runtime keeps making progress while the store load is parked");
    assert!(
        !build.is_finished(),
        "build waits for the parked store load"
    );

    store.open();
    let client = build.await??;
    assert_eq!(
        client
            .session()
            .expect("the stored session was loaded")
            .token()
            .expose_secret(),
        TEST_TOKEN
    );
    assert!(
        client.credentials_persisted(),
        "a freshly loaded record reads as persisted"
    );
    let load_threads = store
        .load_threads
        .lock()
        .expect("load threads lock")
        .clone();
    assert_eq!(load_threads.len(), 1, "exactly one load");
    assert_ne!(
        load_threads[0],
        std::thread::current().id(),
        "the load ran off the runtime"
    );
    Ok(())
}

// ===================== User-Agent reaches every request =====================

/// A `User-Agent` given once to the builder reaches a domain read, not only the login handshake:
/// the whole client shares one transport.
#[tokio::test]
async fn user_agent_is_forwarded_to_domain_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(header("user-agent", LEGACY_USER_AGENT))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .user_agent(Some(LEGACY_USER_AGENT.to_owned()))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    let env = client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}

// ===================== Shared helpers for discovery and session-change tests =====================

/// Mounts `GET /2.2/networks` answering `{"data": {"networks": <networks>}}` exactly once. Mount
/// two of these to make a second discovery see a different account.
async fn mount_networks_once(mock: &MockEero, networks: serde_json::Value) {
    let body = json!({"meta": {"code": 200}, "data": {"networks": networks}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// Mounts the device list of `network_id`, expected `times` times.
async fn mount_devices_of(mock: &MockEero, network_id: &str, times: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/2.2/networks/{network_id}/devices")))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(times)
        .mount(&mock.server)
        .await;
}

// ===================== Integer network ids =====================

/// Some accounts' `/networks` lists give the id as a JSON number. Auto-discovery in
/// `ensure_network_id` renders it as its decimal string, with no `url` to fall back on.
#[tokio::test]
async fn auto_discovery_coerces_an_integer_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": 12_345_678}])).await;
    mount_devices_of(&mock, "12345678", 1).await;

    let client = client(&mock).await;
    client.get_devices(None, false, None, None).await?;

    assert_eq!(client.preferred_network_id().as_deref(), Some("12345678"));
    Ok(())
}

/// The auto-select side effect of `get_networks` coerces the same way, with or without a `url`.
#[tokio::test]
async fn get_networks_auto_select_coerces_an_integer_network_id() -> anyhow::Result<()> {
    for entry in [
        json!({"id": 12_345_678}),
        json!({"id": 12_345_678, "url": "/2.2/networks/12345678"}),
    ] {
        let mock = MockEero::start().await;
        mount_networks_once(&mock, json!([entry.clone()])).await;

        let client = client(&mock).await;
        client.get_networks(false).await?;

        assert_eq!(
            client.preferred_network_id().as_deref(),
            Some("12345678"),
            "{entry}"
        );
    }
    Ok(())
}

/// Python's truthiness rule: an integer `0` counts as absent, so the `url` tail is used.
#[tokio::test]
async fn a_zero_integer_network_id_falls_back_to_the_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(
        &mock,
        json!([{"id": 0, "url": "/2.2/networks/network-0001"}]),
    )
    .await;

    let client = client(&mock).await;
    client.get_networks(false).await?;

    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );
    Ok(())
}

// ===================== Preferred network provenance =====================

/// A network chosen with `set_preferred_network` after a discovery is explicit, so it survives
/// a later session change that would have forgotten the discovered one.
#[tokio::test]
async fn an_explicit_choice_wins_over_an_earlier_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;

    let client = client(&mock).await;
    client.get_networks(false).await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );

    client.set_preferred_network("7");
    client.set_session_token(TEST_TOKEN).await?;

    assert_eq!(client.preferred_network_id().as_deref(), Some("7"));
    Ok(())
}

/// A discovery never overwrites a preferred network that is already set.
#[tokio::test]
async fn discovery_does_not_replace_an_explicit_choice() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;

    let client = client(&mock).await;
    client.set_preferred_network("7");
    client.get_networks(false).await?;

    assert_eq!(client.preferred_network_id().as_deref(), Some("7"));
    Ok(())
}

// ===================== Session changes reset a discovered network =====================

#[derive(Debug, Clone, Copy)]
enum Transition {
    SetSessionToken,
    ClearSessionToken,
    Logout,
}

impl Transition {
    const ALL: [Self; 3] = [Self::SetSessionToken, Self::ClearSessionToken, Self::Logout];

    /// Mounts what the transition itself needs on the wire.
    async fn mount(self, mock: &MockEero) {
        if matches!(self, Self::Logout) {
            Mock::given(method("POST"))
                .and(path("/2.2/logout"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&mock.server)
                .await;
        }
    }

    async fn run(self, client: &Client) -> anyhow::Result<()> {
        match self {
            Self::SetSessionToken => client.set_session_token(TEST_TOKEN).await?,
            Self::ClearSessionToken => client.clear_session_token().await?,
            Self::Logout => {
                assert!(client.logout().await?, "there was a session to log out of");
            }
        }
        Ok(())
    }

    /// Puts a session back at the `EeroApi` layer, so it cannot be this call that resets the
    /// facade state under test. `SetSessionToken` leaves a session in place already.
    async fn restore_session(self, client: &Client) -> anyhow::Result<()> {
        if !matches!(self, Self::SetSessionToken) {
            client.api().auth().set_session_token(TEST_TOKEN).await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn a_session_change_forgets_a_discovered_network_and_rediscovers() -> anyhow::Result<()> {
    for transition in Transition::ALL {
        let mock = MockEero::start().await;
        transition.mount(&mock).await;
        mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;
        mount_networks_once(&mock, json!([{"id": "network-0002"}])).await;
        mount_devices_of(&mock, "network-0002", 1).await;

        let client = client(&mock).await;
        client.get_networks(false).await?;
        assert_eq!(
            client.preferred_network_id().as_deref(),
            Some("network-0001"),
            "{transition:?}"
        );

        transition.run(&client).await?;
        assert_eq!(client.preferred_network_id(), None, "{transition:?}");

        transition.restore_session(&client).await?;
        client.get_devices(None, false, None, None).await?;
        assert_eq!(
            client.preferred_network_id().as_deref(),
            Some("network-0002"),
            "{transition:?}: the next call rediscovers the new account's network"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_explicit_network_survives_every_session_change() -> anyhow::Result<()> {
    for transition in Transition::ALL {
        let mock = MockEero::start().await;
        transition.mount(&mock).await;
        mount_devices_of(&mock, "network-0001", 1).await;

        let client = client(&mock).await;
        client.set_preferred_network("network-0001");

        transition.run(&client).await?;
        assert_eq!(
            client.preferred_network_id().as_deref(),
            Some("network-0001"),
            "{transition:?}"
        );

        transition.restore_session(&client).await?;
        client.get_devices(None, false, None, None).await?;
    }
    Ok(())
}

/// `logout` with no session to log out of reports `false` and changes nothing but the cache,
/// so a discovered network is kept.
#[tokio::test]
async fn logout_without_a_session_keeps_the_discovered_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;

    let client = client(&mock).await;
    client.get_networks(false).await?;
    // Drop the session at the `EeroApi` layer, which leaves the facade state alone.
    client.api().auth().clear_session_token().await?;
    assert!(!client.is_authenticated());

    assert!(!client.logout().await?);

    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );
    Ok(())
}

/// A token that fails validation changes nothing, so neither the cache nor the discovered
/// network is touched.
#[tokio::test]
async fn a_rejected_token_keeps_the_cache_and_the_discovered_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;
    mount_devices_of(&mock, "network-0001", 1).await;

    let client = client(&mock).await;
    client.get_devices(None, false, None, None).await?;

    let err = client
        .set_session_token("")
        .await
        .expect_err("an empty token is rejected");
    assert!(
        matches!(err, Error::Validation { ref field, .. } if field == "token"),
        "{err:?}"
    );

    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );
    // Served from the cache: the device mock above is expected exactly once.
    client.get_devices(None, false, None, None).await?;
    Ok(())
}

/// A failed store write under `Fatal` still leaves the new session in place, so the previous
/// account's network is forgotten as well.
#[tokio::test]
async fn a_failed_store_write_under_fatal_still_forgets_the_discovered_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;
    mount_networks_once(&mock, json!([{"id": "network-0002"}])).await;
    mount_devices_of(&mock, "network-0002", 1).await;
    let store = GateStore::failing();
    let client = client_with_store(&mock, &store, StorageFailures::Fatal).await;

    client.get_networks(false).await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );

    let err = client
        .set_session_token(TEST_TOKEN)
        .await
        .expect_err("Fatal policy surfaces the failed save");
    assert!(matches!(err, Error::Storage(_)), "{err:?}");
    assert_eq!(client.preferred_network_id(), None);

    client.get_devices(None, false, None, None).await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0002")
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_store_clear_under_fatal_still_forgets_the_discovered_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;
    let store = GateStore::failing();
    let client = client_with_store(&mock, &store, StorageFailures::Fatal).await;

    client.get_networks(false).await?;

    let err = client
        .clear_session_token()
        .await
        .expect_err("Fatal policy surfaces the failed clear");
    assert!(matches!(err, Error::Storage(_)), "{err:?}");

    assert!(!client.is_authenticated());
    assert_eq!(client.preferred_network_id(), None);
    Ok(())
}

// ===================== clear_auth_data =====================

/// `clear_auth_data` drops even a network chosen with `set_preferred_network`, and the cache.
#[tokio::test]
async fn clear_auth_data_drops_an_explicit_network_and_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_devices_of(&mock, "network-0001", 2).await;

    let client = client(&mock).await;
    client.set_preferred_network("network-0001");
    client.get_devices(None, false, None, None).await?;

    client.clear_auth_data().await?;

    assert_eq!(client.preferred_network_id(), None);
    assert!(!client.is_authenticated());
    assert!(client.session().is_none());

    client.api().auth().set_session_token(TEST_TOKEN).await?;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_auth_data_drops_a_discovered_network_too() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_networks_once(&mock, json!([{"id": "network-0001"}])).await;

    let client = client(&mock).await;
    client.get_networks(false).await?;
    assert_eq!(
        client.preferred_network_id().as_deref(),
        Some("network-0001")
    );

    client.clear_auth_data().await?;

    assert_eq!(client.preferred_network_id(), None);
    Ok(())
}

#[tokio::test]
async fn clear_auth_data_makes_no_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;

    client.clear_auth_data().await?;

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "{requests:?}");
    Ok(())
}

#[tokio::test]
async fn clear_auth_data_removes_the_stored_record() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store = Arc::new(MemoryStore::new());
    store.save(&Session::from_token(TEST_TOKEN))?;
    let client = Client::builder()
        .base_url(mock.uri())
        .store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");
    assert!(client.is_authenticated(), "the stored session was loaded");

    client.clear_auth_data().await?;

    assert!(!client.is_authenticated());
    assert!(
        !store.load()?.is_valid(),
        "the stored record was removed from the store"
    );
    assert!(client.credentials_persisted());
    Ok(())
}

/// A store that fails to clear is reported, but the in-memory session is already gone, so the
/// facade state is reset regardless.
#[tokio::test]
async fn clear_auth_data_under_fatal_reports_the_store_failure_and_still_resets()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store = GateStore::failing();
    let client = client_with_store(&mock, &store, StorageFailures::Fatal).await;
    client.set_preferred_network("network-0001");

    let err = client
        .clear_auth_data()
        .await
        .expect_err("Fatal policy surfaces the failed clear");

    assert!(matches!(err, Error::Storage(_)), "{err:?}");
    assert!(!client.is_authenticated());
    assert_eq!(client.preferred_network_id(), None);
    Ok(())
}

// ===================== Configuration is validated before the store is read =====================

/// A store that counts how often it is touched and, when built with
/// [`CountingStore::failing`], fails every load, so that touching it changes the outcome of
/// `build` under `StorageFailures::Fatal`.
#[derive(Debug, Default)]
struct CountingStore {
    calls: std::sync::atomic::AtomicUsize,
    fails: bool,
}

impl CountingStore {
    fn failing() -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            fails: true,
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl CredentialStore for CountingStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fails {
            return Err(StorageError::Backend {
                backend: "counting".to_owned(),
                message: "deliberate failure".to_owned(),
            });
        }
        Ok(Session::empty())
    }

    fn save(&self, _session: &Session) -> Result<(), StorageError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn clear(&self) -> Result<(), StorageError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn build_rejects_a_bad_configuration_before_reading_the_store() {
    type Configure = fn(rusteero::client::ClientBuilder) -> rusteero::client::ClientBuilder;
    let cases: [(&str, Configure, &str); 3] = [
        (
            "base_url",
            |builder| builder.base_url("not a url"),
            "base_url",
        ),
        (
            "accept_language",
            |builder| builder.accept_language("bad\nvalue"),
            "X-Accept-Language",
        ),
        // The field name for a bad `User-Agent` is not pinned here: it is spelled by the
        // transport's own validation, which this test deliberately does not duplicate.
        (
            "user_agent",
            |builder| builder.user_agent(Some("bad\nvalue".to_owned())),
            "",
        ),
    ];

    for (label, configure, field) in cases {
        let store = Arc::new(CountingStore::default());
        let builder = configure(
            Client::builder().store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>)),
        );

        let err = builder
            .build()
            .await
            .expect_err("the configuration is invalid");

        match err {
            Error::Validation {
                field: actual_field,
                ..
            } => {
                if !field.is_empty() {
                    assert_eq!(actual_field, field, "{label}");
                }
            }
            other => panic!("{label}: expected a validation error, got {other:?}"),
        }
        assert_eq!(store.calls(), 0, "{label}: the store must not be touched");
    }
}

/// Under `Fatal` a store that fails on load would turn `build` into `Error::Storage`; a bad
/// configuration must win, which only happens if the store is never reached. The field name is
/// not pinned: the transport's own validation spells it.
#[tokio::test]
async fn a_bad_header_under_fatal_is_a_validation_error_and_the_store_is_untouched() {
    type Configure = fn(rusteero::client::ClientBuilder) -> rusteero::client::ClientBuilder;
    let cases: [(&str, Configure); 3] = [
        ("user_agent", |builder| {
            builder.user_agent(Some("bad\r\nvalue".to_owned()))
        }),
        ("accept_language", |builder| {
            builder.accept_language("bad\r\nvalue")
        }),
        ("both", |builder| {
            builder
                .user_agent(Some("bad\r\nvalue".to_owned()))
                .accept_language("bad\r\nvalue")
        }),
    ];

    for (label, configure) in cases {
        let store = Arc::new(CountingStore::failing());
        let builder = configure(
            Client::builder()
                .store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>))
                .storage_failures(StorageFailures::Fatal),
        );

        let err = builder
            .build()
            .await
            .expect_err("the configuration is invalid");

        assert!(
            matches!(err, Error::Validation { .. }),
            "{label}: expected a validation error, got {err:?}"
        );
        assert_eq!(store.calls(), 0, "{label}: the store must not be touched");
    }
}

/// A `base_url` that smuggles another host through userinfo is rejected by the transport's
/// `base_url` validation, before the store is reached (`https://api.example.com@evil.example`
/// parses as host `evil.example` with user `api.example.com`).
#[tokio::test]
async fn a_base_url_with_userinfo_is_rejected_before_the_store_is_read() {
    let store = Arc::new(CountingStore::failing());

    let err = Client::builder()
        .base_url("https://api.example.com@evil.example")
        .store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>))
        .storage_failures(StorageFailures::Fatal)
        .build()
        .await
        .expect_err("userinfo in the base url is rejected");

    assert!(matches!(err, Error::Validation { .. }), "{err:?}");
    assert_eq!(store.calls(), 0);
}

#[tokio::test]
async fn build_reads_the_store_exactly_once_for_a_valid_configuration() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store = Arc::new(CountingStore::default());

    let _client = Client::builder()
        .base_url(mock.uri())
        .store(Some(Arc::clone(&store) as Arc<dyn CredentialStore>))
        .build()
        .await?;

    assert_eq!(store.calls(), 1);
    Ok(())
}

// ===================== A rejected 2xx body is not cached =====================

/// A `2xx` whose body is not a JSON object is an error from the transport, so `get_networks`
/// returns it before the `/account` fallback and before anything is cached or selected.
#[tokio::test]
async fn get_networks_with_a_non_object_body_is_an_error_and_caches_nothing() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {"code": 200}})))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    for attempt in 1..=2 {
        let err = client
            .get_networks(false)
            .await
            .expect_err("a non-object body is an error");
        assert!(
            matches!(
                err,
                Error::Api { status: 200, ref message, .. }
                    if message == "Unexpected JSON response: expected an object"
            ),
            "attempt {attempt}: {err:?}"
        );
    }

    assert_eq!(client.preferred_network_id(), None);
    Ok(())
}
