//! P1.9 transport suite: `Transport::send`/`send_with_query`'s status-code mapping, the 10 MiB
//! response cap, redirect refusal, exact `Cookie` header shape, routing across `ApiVersion::V2_2`
//! / `ApiVersion::V2_3`, and — the largest cluster — the server-driven refresh retry, all against
//! a local `wiremock` server per the crate's testing conventions. `Transport`'s own unit tests (in
//! `src/transport.rs`) already cover `parse_retry_after`, `refresh_signal_detected`, and
//! `render_url` in isolation; this file exercises the same logic end-to-end, over real HTTP,
//! through the public `send`/`send_with_query` entry points only.

mod common;

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Method;
use reqwest::header::COOKIE;
use rusteero::auth::{AuthApi, Session};
use rusteero::consts::MAX_RESPONSE_BYTES;
use rusteero::error::Error;
use rusteero::routes::{ACCOUNT, ApiVersion, Route};
use rusteero::transport::Transport;
use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, session_cookie_for};

// ===================== shared helpers =====================

/// A `PUT /2.3/networks/{network_id}/devices/{device_id}`-shaped route, assembled ad hoc exactly
/// as `routes.rs`'s own doc comment permits: no `Route` for a real device mutation exists yet
/// (phase 3 owns `src/endpoints/`), but `Transport` must already route a `V2_3` write to the
/// `/2.3` host while a plain `V2_2` read stays on `/2.2` — that base-selection logic is this
/// file's responsibility, not phase 3's.
fn device_put_route() -> Route {
    Route {
        method: Method::PUT,
        version: ApiVersion::V2_3,
        path: "networks/{network_id}/devices/{device_id}",
    }
}

/// A `GET /2.2/networks/{network_id}/data_usage`-shaped route: phase 3's `data_usage` endpoint is
/// the one place in the whole port that sends a `GET` with a JSON body attached, a shape `reqwest`
/// supports but this crate has, until this test, only ever verified by reading `reqwest`'s own
/// source.
fn data_usage_route() -> Route {
    Route {
        method: Method::GET,
        version: ApiVersion::V2_2,
        path: "networks/{network_id}/data_usage",
    }
}

/// Builds a `Session` carrying both `token` and `refresh_token`, valid until 2099.
///
/// `MockEero::transport_with_token` (this suite's usual entry point) always builds its session
/// via `Session::from_token`, which never carries a refresh token — exactly the wrong shape for
/// the refresh-cluster tests below, since `Transport::refresh_session` returns
/// `Error::Authentication("No refresh token available")` *before* making any network call when
/// the current session has none. Round-trips through `Session::from_json`'s public,
/// `eero-api`-compatible wire contract rather than reaching into any private field.
fn session_with_refresh_token(token: &str, refresh_token: &str) -> Session {
    let json = format!(
        r#"{{"session_id":"{token}","refresh_token":"{refresh_token}","session_expiry":"2099-01-01T00:00:00"}}"#
    );
    Session::from_json(&json).expect("well-formed stored-session JSON literal")
}

/// A `Transport` pointed at `mock`, seeded with a session that carries both a token and a refresh
/// token (see `session_with_refresh_token`), and no credential store.
fn transport_with_refresh_token(mock: &MockEero, token: &str, refresh_token: &str) -> Transport {
    Transport::builder()
        .base_url(mock.uri())
        .session(Some(session_with_refresh_token(token, refresh_token)))
        .build()
        .expect("a MockServer's own URI is always a valid base URL")
}

/// A `Transport` pointed at `mock`, seeded with a valid session carrying `token` (no refresh
/// token), with both the overall and per-read timeout raised to a generous 60 seconds.
///
/// Used by the 10 MiB body-cap tests below (task item 6, the flake investigation): those tests
/// stream a genuinely large body over a real loopback socket, and the crate's default 10-second
/// `read_timeout` leaves very little margin under heavy CPU contention — a scheduler-starved
/// `tokio` runtime can stall a single chunk read for longer than that even though every byte is
/// already sitting in the local socket buffer, which would misreport as `Error::Timeout`/
/// `Error::Network` instead of the cap-exceeded `Error::Api` these tests assert on. A 60-second
/// budget for a same-host transfer of ~10 MiB is not "tight" by any realistic measure, so this
/// removes the flake risk without weakening what the test actually proves.
fn transport_with_generous_timeouts(mock: &MockEero, token: &str) -> Transport {
    Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(token)))
        .timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("a MockServer's own URI is always a valid base URL")
}

/// Matches a request that carries no `Cookie` header at all.
///
/// Used by the refresh-cluster tests to pin the negative half of task brief finding
/// `auth.md:212-215`: `Transport::refresh_session` passes `token = None` deliberately for both
/// `login/refresh` and `account/refresh`, so neither request may carry the session cookie —
/// every other route in this suite only ever asserts the *positive* shape
/// (`session_cookie()`/`session_cookie_for()`); nothing pinned this absence, so a regression that
/// started attaching the cookie to a refresh call would pass the whole suite silently.
fn no_cookie_header(request: &Request) -> bool {
    !request.headers.contains_key(COOKIE)
}

/// The fixed, non-padding overhead of the JSON document `json_body_of_exact_length` builds
/// (`{"data":{"pad":"..."}}` minus the `"pad"` value's own contents), exposed separately so a
/// caller can recover the resulting `pad` field's length without duplicating (and risking a
/// drift from) the literal prefix/suffix strings.
const JSON_PAD_PREFIX: &str = r#"{"data":{"pad":""#;
const JSON_PAD_SUFFIX: &str = r#""}}"#;

/// Builds a `{"data":{"pad":"..."}}` JSON document whose serialized length is exactly
/// `total_len` bytes — used to pin `read_capped_body`'s strictly-greater-than boundary from both
/// sides: `total_len == MAX_RESPONSE_BYTES` must be accepted, `total_len == MAX_RESPONSE_BYTES +
/// 1` must not.
fn json_body_of_exact_length(total_len: usize) -> String {
    let pad_len = total_len - JSON_PAD_PREFIX.len() - JSON_PAD_SUFFIX.len();
    format!("{JSON_PAD_PREFIX}{}{JSON_PAD_SUFFIX}", "a".repeat(pad_len))
}

/// A minimal `tracing::Subscriber` that records the `path` field of every event into a shared
/// buffer, ignoring everything else. Exists solely so `query_string_is_never_logged_in_the_path`
/// below can prove, over a real `tracing::debug!` call site, that the query string appended in
/// `Transport::execute_raw` never reaches the logged `path` field — the log statement uses
/// `url.path()`, and `url::Url::path()` structurally excludes the query component, but this test
/// pins the *observed* log output rather than trusting that invariant by inspection alone. No new
/// dependency is pulled in: this is a hand-written `tracing_core::Subscriber` impl, not a
/// `tracing-subscriber` `Layer` (`tracing-subscriber` is a dev-dependency already, but wiring a
/// custom `Layer` through its `Registry` would be more machinery for the same result).
struct PathCapturingSubscriber(Arc<Mutex<Vec<String>>>);

/// The `tracing::field::Visit` half of `PathCapturingSubscriber`: every field type this crate's
/// `tracing::debug!` call site can produce ultimately routes to `record_debug` by default (see
/// `tracing_core::field::Value`'s impl for `str`), so overriding only this one method is
/// sufficient to observe the `path` field regardless of how it was recorded.
struct PathVisitor<'a>(&'a mut Vec<String>);

impl tracing::field::Visit for PathVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "path" {
            self.0.push(format!("{value:?}"));
        }
    }
}

impl tracing::Subscriber for PathCapturingSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut paths = self
            .0
            .lock()
            .expect("capture mutex is never held across a panic");
        let mut visitor = PathVisitor(&mut paths);
        event.record(&mut visitor);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

// ===================== 200: raw envelope round-trips byte-identically =====================

#[tokio::test]
async fn status_200_returns_the_raw_envelope_byte_identically() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport.send(&ACCOUNT, &[], None).await?;

    assert_eq!(env.into_value(), fixture_json("account.json"));
    Ok(())
}

// ===================== empty / 204 body =====================

#[tokio::test]
async fn empty_2xx_body_becomes_envelope_over_empty_object() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport.send(&ACCOUNT, &[], None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn status_204_with_a_body_becomes_empty_object_without_ever_parsing_it() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        // A 204 carrying a non-empty, *invalid-JSON* body: the `204` short-circuit in
        // `status_to_envelope` must fire before any attempt to parse it as JSON.
        .respond_with(ResponseTemplate::new(204).set_body_string("this is not json at all"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport.send(&ACCOUNT, &[], None).await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== invalid JSON on a 2xx =====================

#[tokio::test]
async fn invalid_json_on_a_2xx_is_api_error_not_json_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("malformed JSON on a 2xx must not parse as an envelope");

    assert!(matches!(err, Error::Api { status: 200, .. }));
    Ok(())
}

// ===================== 401 =====================

#[tokio::test]
async fn status_401_is_authentication_error_and_is_auth_error_true() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a plain 401 must surface as Error::Authentication");

    assert!(matches!(err, Error::Authentication(_)));
    assert!(err.is_auth_error());
    Ok(())
}

// ===================== 404 =====================

#[tokio::test]
async fn status_404_is_api_error_with_the_python_message_shape_and_is_not_an_auth_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such account"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(message.starts_with("Resource not found: no such account. URL: "));
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== 429 / Retry-After =====================

#[tokio::test]
async fn status_429_with_delta_seconds_retry_after_carries_the_parsed_duration()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("429 must surface as Error::RateLimit");

    assert!(matches!(
        err,
        Error::RateLimit {
            retry_after: Some(duration)
        } if duration == Duration::from_secs(120)
    ));
    Ok(())
}

#[tokio::test]
async fn status_429_with_http_date_retry_after_carries_some_duration() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "Thu, 01 Jan 2099 00:00:00 GMT"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("429 must surface as Error::RateLimit");

    assert!(matches!(
        err,
        Error::RateLimit {
            retry_after: Some(_)
        }
    ));
    Ok(())
}

#[tokio::test]
async fn status_429_with_no_retry_after_header_carries_none() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("429 must surface as Error::RateLimit");

    assert!(matches!(err, Error::RateLimit { retry_after: None }));
    Ok(())
}

#[tokio::test]
async fn status_429_with_a_garbage_retry_after_carries_none_not_a_header_parse_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "not-a-duration"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("429 must surface as Error::RateLimit even with an unparseable header");

    assert!(matches!(err, Error::RateLimit { retry_after: None }));
    Ok(())
}

// ===================== 500 =====================

#[tokio::test]
async fn status_500_is_api_error_carrying_the_truncated_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let long_body = "x".repeat(600);
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500).set_body_string(long_body.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a 500 must surface as Error::Api");

    let Error::Api {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 500);
    assert!(message.len() < long_body.len());
    assert!(message.ends_with("chars total]"));
    Ok(())
}

// ===================== redirect refusal =====================

#[tokio::test]
async fn redirect_is_refused_and_never_reaches_the_location_host() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let redirect_target = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/2.2/account", redirect_target.uri()))
                .insert_header("set-cookie", "s=leaked-token"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    // If `Transport` ever followed the redirect, this is the request it would make — registered
    // with `.expect(0)` so `redirect_target`'s own drop-time verification fails the test the
    // moment it is invoked at all, on any method or path.
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&redirect_target)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a 3xx with a Location header must be refused, not followed");

    let Error::Api {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 302);
    assert!(message.starts_with("Redirect not followed: 302 -> "));
    assert!(!message.contains("leaked-token"));

    let received = redirect_target
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== response size cap =====================

#[tokio::test]
async fn body_larger_than_the_cap_is_an_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let oversized = json_body_of_exact_length(MAX_RESPONSE_BYTES + 1);
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(oversized))
        .expect(1)
        .mount(&mock.server)
        .await;

    // Generous, explicit timeouts (task item 6): see `transport_with_generous_timeouts`'s docs
    // for why the crate's default 10-second read timeout is too tight a margin for a 10 MiB
    // transfer under heavy CI contention.
    let transport = transport_with_generous_timeouts(&mock, TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a body one byte over the cap must be rejected");

    assert!(matches!(err, Error::Api { .. }));
    Ok(())
}

#[tokio::test]
async fn body_exactly_at_the_cap_is_accepted() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let exact = json_body_of_exact_length(MAX_RESPONSE_BYTES);
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(exact))
        .expect(1)
        .mount(&mock.server)
        .await;

    // Generous, explicit timeouts (task item 6): see `transport_with_generous_timeouts`'s docs.
    let transport = transport_with_generous_timeouts(&mock, TEST_TOKEN);
    let env = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect("a body exactly at the cap must be accepted, not rejected");

    assert_eq!(
        env.data()["pad"].as_str().map(str::len),
        Some(MAX_RESPONSE_BYTES - JSON_PAD_PREFIX.len() - JSON_PAD_SUFFIX.len())
    );
    Ok(())
}

// ===================== exact Cookie header =====================

#[tokio::test]
async fn outgoing_request_carries_exactly_the_session_cookie_header() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    transport.send(&ACCOUNT, &[], None).await?;
    Ok(())
}

// ===================== no session =====================

#[tokio::test]
async fn no_session_is_authentication_error_and_the_server_receives_zero_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Deliberately no `Mock` registered: `received_requests()` below proves the guard fires
    // before any I/O, not merely that no *registered* matcher happened to be hit.
    let transport = mock.transport_anonymous();

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("no session configured means the precondition fires before any request");
    assert!(matches!(err, Error::Authentication(ref msg) if msg == "Not authenticated"));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

// ===================== GET with a JSON body =====================

#[tokio::test]
async fn a_get_request_with_a_json_body_actually_arrives_with_that_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let route = data_usage_route();
    let body = json!({ "start": "2024-01-01", "end": "2024-01-31" });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/data_usage"))
        .and(session_cookie())
        .and(body_json(body.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .send(&route, &[("network_id", "network-0001")], Some(body))
        .await?;

    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== ApiVersion routing =====================

#[tokio::test]
async fn v2_3_routes_hit_the_2_3_host_while_v2_2_routes_hit_the_2_2_host() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let device_put = device_put_route();

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    transport.send(&ACCOUNT, &[], None).await?;
    transport
        .send(
            &device_put,
            &[("network_id", "network-0001"), ("device_id", "aa:bb")],
            Some(json!({ "nickname": "renamed" })),
        )
        .await?;
    Ok(())
}

// ===================== query parameters =====================

#[tokio::test]
async fn query_parameters_are_sent_via_send_with_query() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(query_param("since", "1700000000"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .send_with_query(&ACCOUNT, &[], &[("since", "1700000000".to_owned())], None)
        .await?;

    assert_eq!(env.meta().code, Some(200));
    Ok(())
}

#[tokio::test]
async fn query_string_is_never_logged_in_the_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // No `.expect(n)` here: the retry loop below may send this request more than once.
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(query_param("since", "1700000000"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);

    // `tracing`'s per-callsite interest cache is a *process-global* cache shared by every test
    // in this integration-test binary: dozens of the other tests in this file exercise the exact
    // same `tracing::debug!` call site in `Transport::execute_raw` concurrently, with no
    // subscriber of their own, and can race the moment this test installs `subscriber` below,
    // re-caching that callsite's interest as "never" before this test's own request is even
    // sent — silently dropping the event this test needs to observe, with no code-level bug
    // involved. Installing a fresh subscriber (which itself forces `tracing` to rebuild the
    // interest cache, per `tracing_core::callsite`'s own docs) and retrying a bounded number of
    // times turns an occasional lost race into an astronomically unlikely one, without weakening
    // the actual security assertion below.
    let mut logged_paths: Vec<String> = Vec::new();
    for _ in 0..25 {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let subscriber = PathCapturingSubscriber(Arc::clone(&captured));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            transport
                .send_with_query(&ACCOUNT, &[], &[("since", "1700000000".to_owned())], None)
                .await?;
        }
        logged_paths = Arc::try_unwrap(captured)
            .expect("the guard's scope has already ended; no other reference survives")
            .into_inner()
            .expect("capture mutex is never held across a panic");
        if !logged_paths.is_empty() {
            break;
        }
    }

    assert!(
        !logged_paths.is_empty(),
        "expected at least one logged request path to have been captured after repeated attempts"
    );
    for logged in &logged_paths {
        assert!(
            !logged.contains("since") && !logged.contains('?'),
            "logged path leaked the query string: {logged}"
        );
    }
    Ok(())
}

// ===================== the refresh cluster =====================

#[tokio::test]
async fn refresh_success_retries_exactly_once_with_the_new_token() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("initial-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(body_json(json!({ "refresh_token": "old-refresh-token" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "new-token-1" } })
                    .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("new-token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = transport.send(&ACCOUNT, &[], None).await?;
    assert_eq!(env.into_value(), fixture_json("account.json"));
    Ok(())
}

#[tokio::test]
async fn refresh_success_but_retry_still_401s_raises_the_retry_error_with_no_loop()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("initial-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "new-token-2" } })
                    .to_string(),
            ),
        )
        // `.expect(1)`, not `.expect(2)`: even though the retry below also carries the refresh
        // signal, `send_with_query` never re-checks it — the retry result is returned as-is,
        // exactly once, with no second refresh attempt.
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("new-token-2"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a 401 on the retry itself must surface, not loop");
    assert!(matches!(err, Error::Authentication(_)));
    Ok(())
}

#[tokio::test]
async fn refresh_401_with_a_different_meta_error_never_attempts_a_refresh() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "invalid_credentials" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a 401 without the refresh signal must not trigger a refresh");
    assert!(matches!(err, Error::Authentication(_)));
    Ok(())
}

#[tokio::test]
async fn refresh_401_with_a_non_json_body_never_attempts_a_refresh() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(401).set_body_string("plain text, not json"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a non-JSON 401 body must not trigger a refresh");
    assert!(matches!(err, Error::Authentication(_)));
    Ok(())
}

#[tokio::test]
async fn refresh_route_1_404_falls_through_to_route_2_and_retries_on_success() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("initial-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .and(body_json(json!({ "refresh_token": "old-refresh-token" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "new-token-3" } })
                    .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("new-token-3"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = transport.send(&ACCOUNT, &[], None).await?;
    assert_eq!(env.into_value(), fixture_json("account.json"));
    Ok(())
}

#[tokio::test]
async fn refresh_route_1_500_is_terminal_and_route_2_is_never_tried() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie_for("initial-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a terminal non-404 refresh failure must surface the original 401");
    assert!(matches!(err, Error::Authentication(_)));
    Ok(())
}

#[tokio::test]
async fn a_mutating_put_that_401s_is_resent_with_its_original_body_after_a_successful_refresh()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");
    let route = device_put_route();
    let body = json!({ "nickname": "NewName" });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb"))
        .and(session_cookie_for("initial-token"))
        .and(body_json(body.clone()))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            json!({ "meta": { "code": 401, "error": "error.session.refresh" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "new-token-4" } })
                    .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb"))
        .and(session_cookie_for("new-token-4"))
        .and(body_json(body.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = transport
        .send(
            &route,
            &[("network_id", "network-0001"), ("device_id", "aa:bb")],
            Some(body),
        )
        .await?;
    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

// ===================== Transport::refresh_session called directly =====================
//
// Everything above this banner only ever exercises `refresh_session` indirectly, via the 401
// retry inside `send`/`send_with_query`. The tests below call it directly, covering the branches
// documented on `Transport::refresh_session` itself (task item 1).

#[tokio::test]
async fn refresh_session_direct_call_with_no_refresh_token_is_the_universal_real_world_case()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Deliberately no `Mock` registered at all: `received_requests()` below proves the
    // precondition fires before any I/O, not merely that no *registered* matcher happened to be
    // hit. `Session::from_token` never carries a refresh token, matching every real
    // login/verify session (port plan §1.3(5)) — this is the universal, real-world shape of this
    // call, not an edge case.
    let transport = mock.transport_with_token(TEST_TOKEN);

    let err = transport
        .refresh_session()
        .await
        .expect_err("no refresh token means the precondition fires before any network call");
    assert!(matches!(
        err,
        Error::Authentication(ref msg) if msg == "No refresh token available"
    ));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

#[tokio::test]
async fn refresh_session_direct_call_falls_through_a_404_to_route_2_and_installs_the_new_token()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .and(body_json(json!({ "refresh_token": "old-refresh-token" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "direct-new-token" } })
                    .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let refreshed = transport.refresh_session().await?;
    assert!(refreshed, "route 2's success must report Ok(true)");

    let session = transport
        .session()
        .expect("a successful refresh leaves a session installed");
    assert_eq!(session.token().expose_secret(), "direct-new-token");
    Ok(())
}

#[tokio::test]
async fn refresh_session_direct_call_terminal_500_never_tries_route_2_and_clears_the_session()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let refreshed = transport.refresh_session().await?;
    assert!(
        !refreshed,
        "a terminal non-404 refresh failure reports Ok(false), it never propagates"
    );

    // `Transport::refresh_session`'s own docs: a terminal failure clears the local session
    // (persisting the clear, best-effort) rather than leaving the stale token installed.
    let session = transport
        .session()
        .expect("refresh_session installs Session::empty() on this arm, not None");
    assert!(!session.is_valid());
    assert!(!transport.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn refresh_session_response_missing_session_token_returns_false_and_changes_nothing()
-> anyhow::Result<()> {
    // Pins `Transport::refresh_session`'s own documented contract for this arm exactly:
    // "`SESSION_TOKEN_KEY` missing or empty: Python returns `False` without clearing or
    // persisting anything" — unlike the terminal-error arm above, this must leave the existing
    // session (token *and* refresh token) completely untouched.
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({ "meta": { "code": 200 }, "data": {} }).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let refreshed = transport.refresh_session().await?;
    assert!(
        !refreshed,
        "a 200 refresh response with no `session_token` in `data` must return Ok(false)"
    );

    let session = transport
        .session()
        .expect("this arm changes nothing: the prior session must still be installed");
    assert_eq!(session.token().expose_secret(), "initial-token");
    assert_eq!(
        session
            .refresh_token()
            .expect("refresh token must be untouched")
            .expose_secret(),
        "old-refresh-token"
    );
    Ok(())
}

#[tokio::test]
async fn refresh_session_response_with_a_new_refresh_token_replaces_the_old_one()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({
                    "meta": { "code": 200 },
                    "data": {
                        "session_token": "rotated-token",
                        "refresh_token": "rotated-refresh-token"
                    }
                })
                .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let refreshed = transport.refresh_session().await?;
    assert!(refreshed);

    let session = transport
        .session()
        .expect("a successful refresh leaves a session installed");
    assert_eq!(session.token().expose_secret(), "rotated-token");
    assert_eq!(
        session
            .refresh_token()
            .expect("a new refresh token in the response replaces the old one")
            .expose_secret(),
        "rotated-refresh-token"
    );
    Ok(())
}

// ===================== refresh requests never carry a Cookie header =====================

#[tokio::test]
async fn refresh_requests_carry_no_cookie_header_on_either_route() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = transport_with_refresh_token(&mock, "initial-token", "old-refresh-token");

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(no_cookie_header)
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/refresh"))
        .and(no_cookie_header)
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({ "meta": { "code": 200 }, "data": { "session_token": "no-cookie-token" } })
                    .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    // If either request actually carried a `Cookie` header, the `no_cookie_header` matcher above
    // would never match it: wiremock would then have no responder for that request, and each
    // mock's own `.expect(1)` verification (checked when `mock.server` is dropped at the end of
    // this test) would fail with zero recorded matches instead of one.
    let refreshed = transport.refresh_session().await?;
    assert!(refreshed);
    Ok(())
}

// ===================== AuthApi::refresh_session =====================

#[tokio::test]
async fn auth_api_refresh_session_with_no_refresh_token_delegates_and_makes_no_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let auth = mock.auth_api_with_token(TEST_TOKEN);

    let err = auth.refresh_session().await.expect_err(
        "AuthApi::refresh_session must surface Transport::refresh_session's own precondition error",
    );
    assert!(matches!(
        err,
        Error::Authentication(ref msg) if msg == "No refresh token available"
    ));

    let received = mock
        .server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_refresh_session_reports_success_and_updates_the_wrapped_sessions_token()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let auth = AuthApi::new(transport_with_refresh_token(
        &mock,
        "initial-token",
        "old-refresh-token",
    ));

    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({ "meta": { "code": 200 }, "data": { "session_token": "authapi-new-token" } })
                .to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let refreshed = auth.refresh_session().await?;
    assert!(refreshed);

    let session = auth
        .session()
        .expect("a successful refresh leaves a session installed");
    assert_eq!(session.token().expose_secret(), "authapi-new-token");
    Ok(())
}

// ===================== map_reqwest_error: Timeout and Network =====================

#[tokio::test]
async fn a_response_slower_than_the_configured_timeout_is_error_timeout() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A ~17x margin between the configured timeout (300 ms) and the server's delay (5 s):
    // generous enough that a heavily loaded CI host cannot accidentally make the *client* time
    // out before the delay even starts to matter, nor accidentally let the delayed response
    // arrive before the client gives up (task item 6: no assumption that a delay is tight).
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("account.json"))
                .set_delay(Duration::from_secs(5)),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .timeout(Duration::from_millis(300))
        .read_timeout(Duration::from_millis(300))
        .build()
        .expect("a MockServer's own URI is always a valid base URL");

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a response far slower than the configured timeout must time out");
    assert!(
        matches!(err, Error::Timeout),
        "expected Error::Timeout, got {err:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_closed_local_port_surfaces_as_error_network() -> anyhow::Result<()> {
    // Fully offline and deterministic: no external host, no DNS lookup. Binding to port 0 asks
    // the OS for an ephemeral free port; dropping the listener immediately frees it again while
    // keeping the number reserved from the OS's short-term reuse pool long enough for the
    // following connection attempt, which reliably gets an immediate connection-refused rather
    // than a connect timeout.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let transport = Transport::builder()
        .base_url(format!("http://127.0.0.1:{port}"))
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .expect("a loopback base URL is always valid");

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("connecting to a closed local port must fail at the transport level");
    assert!(
        matches!(err, Error::Network(_)),
        "expected Error::Network, got {err:?}"
    );
    Ok(())
}

// ===================== malformed JSON on a 2xx never leaks a credential in Display =====================

#[tokio::test]
async fn malformed_json_2xx_body_carrying_a_session_token_never_leaks_it_in_the_error_display()
-> anyhow::Result<()> {
    // Deliberately truncated / malformed: the exact shape a cut-off `login/refresh` or
    // `account/refresh` response would have, legitimately carrying a live credential inside a
    // body that `status_to_envelope`'s invalid-JSON-on-2xx arm turns into an `Error::Api`.
    let credential_carrying_body = concat!(
        r#"{"meta":{"code":200},"data":{"session_token":"eyJsecret-session-value","#,
        r#""refresh_token":"eyJsecret-refresh-value"#,
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(credential_carrying_body).is_err(),
        "sanity: the fixture must actually be malformed JSON, matching the real hazard"
    );

    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(credential_carrying_body))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("malformed JSON on a 2xx must not parse as an envelope");

    let rendered = err.to_string();
    assert!(
        !rendered.contains("eyJsecret-session-value"),
        "leaked: {rendered}"
    );
    assert!(
        !rendered.contains("eyJsecret-refresh-value"),
        "leaked: {rendered}"
    );
    assert!(
        !rendered.contains("eyJ"),
        "leaked a token-shaped prefix: {rendered}"
    );
    Ok(())
}

// ===================== injected redirect-following client (finding F4) =====================

#[tokio::test]
async fn injected_redirect_following_client_never_returns_the_followed_hops_body()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/2.2/elsewhere", mock.uri())),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/elsewhere"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    // A caller-supplied client that keeps reqwest's default redirect-following policy — exactly
    // the hazard `TransportBuilder::http`'s `# Warning` section documents. `src/transport.rs`'s
    // own unit test (`injected_client_that_follows_redirects_is_still_refused`) already pins the
    // error *variant* in isolation; this integration-level test additionally pins the exact
    // message shape and, most importantly, proves the followed hop's fixture body never reaches
    // a caller as a successful envelope.
    //
    // Note this is deliberately *not* the same contract as the `Policy::none()` path (see
    // `redirect_is_refused_and_never_reaches_the_location_host` above): by the time `Transport`
    // observes this response the injected client has already followed the redirect over the
    // network, so the status here is the *final* hop's (200), and the message says so honestly
    // ("followed", not "not followed") — see `execute_raw`'s doc comment on the `response.url()
    // != &url` guard.
    let following_client = reqwest::Client::builder()
        .build()
        .expect("a default reqwest client always builds");

    let transport = Transport::builder()
        .base_url(mock.uri())
        .http(following_client)
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .expect("builds with an injected client");

    let err = transport
        .send(&ACCOUNT, &[], None)
        .await
        .expect_err("a followed redirect must surface as an error, not the hop's body");

    let Error::Api {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    // The security property this test guards: the hop's fixture body (`account.json`) must never
    // reach the caller as a successful envelope. Asserting `Err` above already proves that; this
    // additionally proves the message never embeds the body either.
    assert_eq!(*status, 200, "the final hop's status, not the original 302");
    assert!(
        message.starts_with("Redirect followed by a caller-supplied client: "),
        "unexpected message: {message}"
    );
    assert!(
        message.contains(&format!("{}/2.2/account", mock.uri())),
        "message must name the originally requested URL: {message}"
    );
    assert!(
        message.contains(&format!("{}/2.2/elsewhere", mock.uri())),
        "message must name the final, followed-to URL: {message}"
    );
    assert!(
        !message.contains(&fixture("account.json")),
        "message must never embed the followed hop's body: {message}"
    );
    Ok(())
}
