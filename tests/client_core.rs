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
//! that a failed `logout` still clears the cache (security review finding F1).
//!
//! `get_devices` is used throughout purely as a convenient, already-cached vehicle to exercise
//! these generic `Client`/`Cache` mechanics — none of the assertions here are about `DevicesApi`
//! business logic itself (that lives in `endpoints_devices.rs`/`client_devices.rs`). The
//! `/account` fallback of `get_networks`, and `get_networks`/`get_account`-specific caching, are
//! judged to be networks-domain tests and live in `client_networks.rs` instead — see that file's
//! module docs for the reasoning.
//!
//! Every caching test asserts on the wiremock `.expect(n)` call count, not just the returned
//! value — a caching test that only checks the envelope is not testing caching at all (see
//! the crate's testing conventions' "Assertion Patterns"). `logout_...`/`clear_session_token_...`
//! restore a session directly at the `EeroApi` layer via [`Client::api`] rather than through
//! `Client::set_session_token` — deliberately, so that restoring the ability to make a second
//! request never itself clears the cache and masks a regression in the very call site under test.

mod common;

use std::time::Duration;

use reqwest::header::COOKIE;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

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
    let first = client.get_devices(Some("network-0001"), false).await?;
    let second = client.get_devices(Some("network-0001"), false).await?;

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
    client.get_devices(Some("network-0001"), true).await?;
    client.get_devices(Some("network-0001"), true).await?;
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

    // A `reqwest::Client` with neither of the crate's default request/read timeouts configured
    // (see `Client::builder().http(..)`'s own docs for this escape hatch). Those timeouts are
    // implemented as `tokio::time` sleeps racing the real response; `tokio::time::pause()`'s
    // documented auto-advance behaviour ("if the runtime has no work to do, the clock is
    // auto-advanced to the next pending timer") can fire one of those sleeps the instant the
    // `advance()` call below leaves the runtime transiently idle mid-request — observed in
    // practice as a flaky, spurious `Error::Timeout` (or an outright dropped connection) on the
    // second call. Dropping both timeouts removes that race entirely; the real, unmocked socket
    // I/O this test still performs is unaffected by the paused clock either way.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("a client with no timeouts configured always builds");
    let client = Client::builder()
        .http(http)
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .cache_ttl(Duration::from_secs(30))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    client.get_devices(Some("network-0001"), false).await?;

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;

    client.get_devices(Some("network-0001"), false).await?;
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
    client.get_devices(Some("network-0001"), false).await?;
    client.get_devices(Some("network-0001"), false).await?;
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
    let first = client.get_devices(Some("network-0001"), false).await?;
    let second = client.get_devices(Some("network-0001"), false).await?;

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
    let a_first = client.get_devices(Some("net-a"), false).await?;
    let b_first = client.get_devices(Some("net-b"), false).await?;
    let a_second = client.get_devices(Some("net-a"), false).await?;
    let b_second = client.get_devices(Some("net-b"), false).await?;

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

    let env = client.get_devices(Some("explicit-net"), false).await?;
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

    let env = client.get_devices(None, false).await?;
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
    let env = client.get_devices(None, false).await?;

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
    let env = client.get_devices(None, false).await?;

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
        .get_devices(None, false)
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
    client.get_devices(Some("network-0001"), false).await?;
    client.clear_cache();
    client.get_devices(Some("network-0001"), false).await?;
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
    client.get_devices(Some("network-0001"), false).await?;

    client.logout().await?;

    // `logout` also clears the in-memory session, so a session must be restored before this
    // cached getter can be called again. Restore it directly at the `EeroApi` layer (the escape
    // hatch `Client::api` documents) rather than via `Client::set_session_token`, which
    // deliberately clears the cache itself (covered separately below) — if that shortcut were
    // used here instead, this test could pass even if `Client::logout` never cleared the cache
    // at all.
    client.api().auth().set_session_token(TEST_TOKEN)?;
    client.get_devices(Some("network-0001"), false).await?;
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
    client.get_devices(Some("network-0001"), false).await?;

    client.set_session_token(TEST_TOKEN)?;
    client.get_devices(Some("network-0001"), false).await?;
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
    client.get_devices(Some("network-0001"), false).await?;

    client.clear_session_token()?;
    // `clear_session_token` nulls the in-memory session token itself, so — exactly as in the
    // `logout` test above — restore it via the `EeroApi` layer directly, never through
    // `Client::set_session_token`, so this call cannot be the one masking a missing cache clear
    // in `clear_session_token` itself.
    client.api().auth().set_session_token(TEST_TOKEN)?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}

/// **F1** (security review) / v8.0.4 logout contract: `AuthApi::logout` clears the in-memory
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

    // Before the F1 fix this served the pre-logout envelope straight from cache (`Ok`), with no
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

    let env = client.get_devices(Some("network-0001"), false).await?;
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
    let env = client.get_devices(Some("network-0001"), false).await?;
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

    let env = client.get_devices(Some("network-0001"), false).await?;
    assert_eq!(env.as_value(), &fixture_json("devices.json"));
    Ok(())
}
