//! P1.9 transport suite: `Transport::send`/`send_with_query`'s status-code mapping, the 10 MiB
//! response cap, redirect refusal, exact `Cookie` header shape, routing across `ApiVersion::V2_2`
//! / `ApiVersion::V2_3`, and — the largest cluster — the server-driven refresh retry, all against
//! a local `wiremock` server per `.claude/rules/testing.md`. `Transport`'s own unit tests (in
//! `src/transport.rs`) already cover `parse_retry_after`, `refresh_signal_detected`, and
//! `render_url` in isolation; this file exercises the same logic end-to-end, over real HTTP,
//! through the public `send`/`send_with_query` entry points only.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Method;
use rusteero::auth::Session;
use rusteero::consts::MAX_RESPONSE_BYTES;
use rusteero::error::Error;
use rusteero::routes::{ACCOUNT, ApiVersion, Route};
use rusteero::transport::Transport;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

    let transport = mock.transport_with_token(TEST_TOKEN);
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

    let transport = mock.transport_with_token(TEST_TOKEN);
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
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(query_param("since", "1700000000"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = PathCapturingSubscriber(Arc::clone(&captured));

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        transport
            .send_with_query(&ACCOUNT, &[], &[("since", "1700000000".to_owned())], None)
            .await?;
    }

    let logged_paths = captured
        .lock()
        .expect("capture mutex is never held across a panic");
    assert!(
        !logged_paths.is_empty(),
        "expected at least one logged request path to have been captured"
    );
    for logged in logged_paths.iter() {
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
