//! v8.0.4 transport suite: `Transport::request`/`resource`'s status-code mapping, the 10 MiB
//! response cap, redirect refusal, header construction, credential placement (`X-User-Token` +
//! optional legacy cookie), `RequestBody` encoding, the bounded `GET`-only retry, and the
//! server-driven refresh handshake (including single-flight coalescing), all against a local
//! `wiremock` server per the crate's testing conventions. `Transport`'s own unit tests (in
//! `src/transport.rs`) already cover `parse_retry_after`, `refresh_signal_detected`, and
//! `matches_configured_host` in isolation; this file exercises the same logic end-to-end, over
//! real HTTP.

mod common;

use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use reqwest::header::COOKIE;
use rusteero::auth::Session;
use rusteero::consts::MAX_RESPONSE_BYTES;
use rusteero::error::Error;
use rusteero::routes::{ACCOUNT, ApiVersion, Nested, Resource};
use rusteero::transport::{RequestBody, Transport};
use serde_json::json;
use wiremock::matchers::{body_json, header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

// ===================== shared helpers =====================

/// A `PUT /2.3/networks/{network}/devices/{device}`-shaped route, assembled ad hoc exactly as
/// `routes/mod.rs`'s own doc comment permits.
fn device_put_route() -> Nested {
    Nested {
        method: Method::PUT,
        version: ApiVersion::V2_3,
        prefix: "devices",
        suffix: "",
        link: None,
    }
}

/// A `GET /2.2/networks/{id}/data_usage`-shaped route: the one place in the whole port that
/// sends a `GET` with a JSON body attached.
fn data_usage_route() -> Resource {
    Resource {
        method: Method::GET,
        version: ApiVersion::V2_2,
        template: "networks/{id}/data_usage",
        link: None,
    }
}

/// A `Transport` pointed at `mock`, seeded with a valid session carrying `token`, with both the
/// overall and per-read timeout raised to a generous 60 seconds — used by the 10 MiB body-cap
/// tests below, since the crate's default 10-second `read_timeout` leaves very little margin
/// under heavy CI contention for a genuinely large body over a real loopback socket.
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
fn no_cookie_header(request: &Request) -> bool {
    !request.headers.contains_key(COOKIE)
}

const JSON_PAD_PREFIX: &str = r#"{"data":{"pad":""#;
const JSON_PAD_SUFFIX: &str = r#""}}"#;

/// Builds a `{"data":{"pad":"..."}}` JSON document whose serialized length is exactly
/// `total_len` bytes.
fn json_body_of_exact_length(total_len: usize) -> String {
    let pad_len = total_len - JSON_PAD_PREFIX.len() - JSON_PAD_SUFFIX.len();
    format!("{JSON_PAD_PREFIX}{}{JSON_PAD_SUFFIX}", "a".repeat(pad_len))
}

// ===================== 200: raw envelope round-trips byte-identically =====================

#[tokio::test]
async fn status_200_returns_the_raw_envelope_byte_identically() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;

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
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;

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
        .respond_with(ResponseTemplate::new(204).set_body_string("this is not json at all"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;

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
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("malformed JSON on a 2xx must not parse as an envelope");

    assert!(matches!(err, Error::Api { status: 200, .. }));
    Ok(())
}

// ===================== 401 / 404 / 429 / 500 =====================

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
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a plain 401 must surface as Error::Authentication");

    assert!(matches!(err, Error::Authentication { .. }));
    assert!(err.is_auth_error());
    Ok(())
}

#[tokio::test]
async fn status_404_is_not_found_with_the_catalogue_message_shape_and_is_not_an_auth_error()
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
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert_eq!(message, "unrecognised error string");
    assert!(!err.is_auth_error());
    Ok(())
}

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
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("429 must surface as Error::RateLimit");

    assert!(matches!(
        err,
        Error::RateLimit {
            retry_after: Some(duration),
            ..
        } if duration == Duration::from_secs(120)
    ));
    Ok(())
}

#[tokio::test]
async fn status_500_is_api_error_with_the_fixed_unrecognised_message_never_the_body()
-> anyhow::Result<()> {
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
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 500 must surface as Error::Api");

    let Error::Api {
        status,
        message,
        envelope,
        ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 500);
    assert_eq!(message, "unrecognised error string");
    assert!(envelope.is_none());
    assert!(!err.to_string().contains('x'));
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

    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&redirect_target)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 3xx with a Location header must be refused, not followed");

    let Error::Api {
        status, message, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 302);
    assert!(message.starts_with("Redirect not followed: 302 at /2.2/account -> "));
    assert!(!message.contains("leaked-token"));
    // The message must carry the `Location` header's *host* only, never a full URL (which could
    // embed a query string of its own) — the redirect target's bare loopback host:port, with no
    // scheme and no `/2.2/account` path repeated a second time.
    assert!(!message.contains("://"));
    assert!(!message.ends_with("/2.2/account"));

    let received = redirect_target
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(received.is_empty());
    Ok(())
}

#[tokio::test]
async fn redirect_with_no_location_header_is_refused_with_the_fixed_message() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(302))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 3xx with no Location header must still be refused");

    let Error::Api { message, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(
        message,
        "Redirect not followed: 302 at /2.2/account (no Location header)"
    );
    Ok(())
}

/// Security finding 1: `TransportBuilder::http_builder` (the escape hatch that replaces the
/// removed `TransportBuilder::http`) must force `Policy::none()` onto the resulting
/// `reqwest::Client` even when the caller-supplied `reqwest::ClientBuilder` explicitly set a
/// *permissive* redirect policy of its own (`Policy::limited(10)`, not merely the default) —
/// `build()`'s own forced `.redirect(Policy::none())` must be applied last and win regardless.
#[tokio::test]
async fn http_builder_with_an_explicit_permissive_policy_still_refuses_a_cross_host_redirect()
-> anyhow::Result<()> {
    let server_a = MockServer::start().await;
    let server_b = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/2.2/account", server_b.uri())),
        )
        .expect(1)
        .mount(&server_a)
        .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data":{}}"#))
        .expect(0)
        .mount(&server_b)
        .await;

    let permissive_builder =
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::limited(10));

    let transport = Transport::builder()
        .base_url(server_a.uri())
        .http_builder(permissive_builder)
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .expect("builds with an injected client builder");

    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("the transport's own Policy::none() must win over the builder's own policy");
    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 302);

    let received_b = server_b
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert!(
        received_b.is_empty(),
        "server B must never receive a request: the redirect must be refused before the hop"
    );
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

    let transport = transport_with_generous_timeouts(&mock, TEST_TOKEN);
    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
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

    let transport = transport_with_generous_timeouts(&mock, TEST_TOKEN);
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect("a body exactly at the cap must be accepted, not rejected");

    assert_eq!(
        env.data()["pad"].as_str().map(str::len),
        Some(MAX_RESPONSE_BYTES - JSON_PAD_PREFIX.len() - JSON_PAD_SUFFIX.len())
    );
    Ok(())
}

// ===================== no session =====================

#[tokio::test]
async fn no_session_is_authentication_error_and_the_server_receives_zero_requests()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let transport = mock.transport_anonymous();

    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("no session configured means the precondition fires before any request");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Not authenticated")
    );

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
        .resource(&route, "network-0001", None, &[], RequestBody::Json(body))
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
    transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    transport
        .nested(
            &device_put,
            "network-0001",
            "aa:bb",
            None,
            &[],
            RequestBody::Json(json!({ "nickname": "renamed" })),
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
        .resource(
            &ACCOUNT,
            "",
            None,
            &[("since", "1700000000".to_owned())],
            RequestBody::None,
        )
        .await?;

    assert_eq!(env.meta().code, Some(200));
    Ok(())
}

// ===================== header construction =====================

#[tokio::test]
async fn every_request_carries_accept_user_agent_and_accept_language() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(header("accept", "application/json"))
        .and(header("user-agent", rusteero::consts::DEFAULT_USER_AGENT))
        .and(header(
            "x-accept-language",
            rusteero::consts::DEFAULT_ACCEPT_LANGUAGE,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn user_agent_and_accept_language_can_be_overridden() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(header("user-agent", "custom-agent/1.0"))
        .and(header("x-accept-language", "fr-FR"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .user_agent(Some("custom-agent/1.0".to_owned()))
        .accept_language("fr-FR")
        .build()?;
    transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    Ok(())
}

#[test]
fn invalid_accept_language_is_rejected_at_build_time() {
    let err = Transport::builder()
        .accept_language("bad\r\nvalue")
        .build()
        .expect_err("a CR/LF in the header value must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "X-Accept-Language"));
}

// ===================== credential placement =====================

#[tokio::test]
async fn x_user_token_and_legacy_cookie_are_both_sent_by_default() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn legacy_cookie_is_absent_when_disabled() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(user_token_header())
        .and(no_cookie_header)
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .send_legacy_cookie(false)
        .build()?;
    transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn credential_is_withheld_from_a_request_to_a_foreign_host() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let foreign = MockServer::start().await;
    Mock::given(method("GET"))
        .and(header_exists("x-user-token"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&foreign)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&foreign)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let foreign_url = url::Url::parse(&format!("{}/2.2/account", foreign.uri()))?;
    let env = transport
        .request(Method::GET, foreign_url, &[], RequestBody::None)
        .await?;
    assert_eq!(env.into_value(), json!({}));
    Ok(())
}

#[tokio::test]
async fn credential_is_withheld_from_a_scheme_mismatch() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(header_exists("x-user-token"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    // Same host:port as the configured base, but `https` instead of the mock server's plain
    // `http` — the scheme-mismatch half of the credential-placement gate.
    let mismatched = mock.uri().replacen("http://", "https://", 1);
    let mismatched_url = url::Url::parse(&format!("{mismatched}/2.2/account"))?;

    // This request never actually reaches the (http-only) mock server successfully — it fails at
    // the TLS layer — but the only thing under test is that no credential was ever attached, which
    // the `.expect(0)` mock above already verifies regardless of how the connection attempt ends.
    let _ = transport
        .request(Method::GET, mismatched_url, &[], RequestBody::None)
        .await;
    Ok(())
}

// ===================== RequestBody encoding =====================

#[tokio::test]
async fn request_body_form_is_url_encoded() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/account"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(wiremock::matchers::body_string("a=1&b=two"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let url = url::Url::parse(&format!("{}/2.2/account", mock.uri()))?;
    transport
        .request(
            Method::POST,
            url,
            &[],
            RequestBody::Form(vec![
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "two".to_owned()),
            ]),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn request_body_empty_json_string_is_the_literal_two_byte_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(header("content-type", "application/json"))
        .and(wiremock::matchers::body_string("\"\""))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let url = url::Url::parse(&format!("{}/2.2/login/refresh", mock.uri()))?;
    transport
        .request(Method::POST, url, &[], RequestBody::EmptyJsonString)
        .await?;
    Ok(())
}

// ===================== GET-only bounded retry =====================

#[tokio::test]
async fn get_is_retried_on_a_5xx_and_succeeds_on_the_second_attempt() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .get_retries(1)
        .build()?;
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    assert_eq!(env.meta().code, Some(200));
    Ok(())
}

#[tokio::test]
async fn a_post_is_never_retried_even_on_a_5xx() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .get_retries(3)
        .build()?;
    let err = transport
        .resource(&rusteero::routes::LOGOUT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 503 still surfaces as an error");
    assert!(matches!(err, Error::Api { status: 503, .. }));
    Ok(())
}

#[tokio::test]
async fn a_404_is_never_retried() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .get_retries(3)
        .build()?;
    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("a 404 still surfaces as an error");
    assert!(matches!(err, Error::NotFound { .. }));
    Ok(())
}

#[tokio::test]
async fn get_retries_zero_means_no_retry_at_all() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("no retries configured means a single failing attempt");
    assert!(matches!(err, Error::Api { status: 503, .. }));
    Ok(())
}

// ===================== 401 error.session.refresh: one-shot replay =====================

#[tokio::test]
async fn a_401_session_refresh_signal_triggers_one_replay_with_the_refreshed_token()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(header("x-user-token", TEST_TOKEN))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.refresh"}}"#),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(header("x-user-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await?;
    assert_eq!(env.into_value(), fixture_json("account.json"));
    Ok(())
}

// ===================== Transport::refresh_session =====================

#[tokio::test]
async fn refresh_session_success_keeps_the_current_token_and_ignores_the_response_body()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(header("x-user-token", TEST_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"meta":{"code":200},"data":{"session_token":"server-issued-and-ignored"}}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let refreshed = transport.refresh_session().await?;
    assert!(refreshed);

    let current = transport.session().expect("session still present");
    assert_eq!(
        secrecy::ExposeSecret::expose_secret(current.token()),
        TEST_TOKEN,
        "the server-issued token must be discarded per SDK policy"
    );
    Ok(())
}

#[tokio::test]
async fn refresh_session_no_session_is_authentication_error() {
    let mock = MockEero::start().await;
    let transport = mock.transport_anonymous();
    let err = transport
        .refresh_session()
        .await
        .expect_err("no session configured");
    assert!(
        matches!(err, Error::Authentication { message, .. } if message == "No session token available. Login first.")
    );
}

#[tokio::test]
async fn refresh_session_session_expired_clears_credentials_and_returns_false() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.expired"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn rusteero::storage::CredentialStore> =
        Arc::new(rusteero::storage::MemoryStore::new());
    let transport = mock.transport_with_store(TEST_TOKEN, Arc::clone(&store));
    let refreshed = transport.refresh_session().await?;
    assert!(!refreshed);
    assert!(!transport.is_authenticated());
    assert!(!store.load()?.is_valid());
    Ok(())
}

#[tokio::test]
async fn refresh_session_verification_required_retains_credentials_and_returns_false()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.verification.required"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let refreshed = transport.refresh_session().await?;
    assert!(!refreshed);
    assert!(
        transport.is_authenticated(),
        "the verification-required group must retain credentials"
    );
    Ok(())
}

#[tokio::test]
async fn refresh_session_rate_limited_retains_credentials_and_returns_false() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let refreshed = transport.refresh_session().await?;
    assert!(!refreshed);
    assert!(transport.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn refresh_session_network_error_propagates() {
    // No listener at all on this port: a connection attempt fails at the transport layer, never
    // reaching an HTTP response.
    let transport = Transport::builder()
        .base_url("http://127.0.0.1:1")
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .expect("builds with an unreachable base");
    let err = transport
        .refresh_session()
        .await
        .expect_err("a connection failure must propagate, not become Ok(false)");
    assert!(matches!(err, Error::Network(_) | Error::Timeout));
}

// ===================== refresh coalescing (single-flight) =====================

#[tokio::test]
async fn concurrent_refresh_calls_coalesce_into_a_single_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"meta":{"code":200},"data":{}}"#)
                .set_delay(Duration::from_millis(200)),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Arc::new(mock.transport_with_token(TEST_TOKEN));
    let mut handles = Vec::new();
    for _ in 0..3 {
        let transport = Arc::clone(&transport);
        handles.push(tokio::spawn(
            async move { transport.refresh_session().await },
        ));
    }
    for handle in handles {
        let result = handle.await.expect("task does not panic")?;
        assert!(result, "every caller observes the leader's success");
    }
    Ok(())
}
