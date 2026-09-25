//! P1.9 auth suite: the interactive login handshake (`LoginFlow`/`PendingLogin`) and the
//! network-facing half of `AuthApi` (`login`/`resend`/`verify`, `logout`, `refresh`-adjacent
//! local checks, the three `clear_*`/`set_session_token` scopes), all against a local wiremock
//! server per the crate's testing conventions.
//!
//! Two tests below (`pending_login_verify_without_set_cookie_uses_the_login_token_d16_branch_a`
//! and `pending_login_verify_with_a_fresh_set_cookie_uses_the_new_token_d16_branch_b`) pin the
//! one open question phase 1 has not yet resolved by live capture: whether the real server sets
//! a fresh `Set-Cookie: s=...` on `login/verify` (the port plan §7.2,
//! decision D-16). Both branches are exercised so that whichever the live capture confirms,
//! `resolve_session_token` (`src/auth/flow.rs`) already has a passing test for it.

mod common;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, session_cookie_for};
use rusteero::auth::flow::PendingLogin;
use rusteero::auth::{AuthApi, Session};
use rusteero::consts::SESSION_LIFETIME_DAYS;
use rusteero::error::Error;
use rusteero::storage::{CredentialStore, MemoryStore};
use rusteero::transport::Transport;

/// The `data.user_token` value baked into `fixtures/login.json`, read out dynamically so this
/// file never hardcodes a value the fixture's owner could change out from under it.
fn login_fixture_token() -> String {
    fixture_json("login.json")["data"]["user_token"]
        .as_str()
        .expect("fixtures/login.json always carries a string data.user_token")
        .to_owned()
}

/// Mounts the shared `/2.2/login` mock (responding with `fixtures/login.json`, no assertion on
/// the request body beyond method/path — that exact shape is `login_flow_start_...`'s job
/// below) and completes `LoginFlow::start`, handing back the resulting `PendingLogin` together
/// with the login token `fixtures/login.json` carries. Every resend/verify test below needs
/// both, so factoring this out keeps each of them focused on its own request assertion.
async fn start_pending_login(mock: &MockEero) -> anyhow::Result<(PendingLogin, String)> {
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let pending = mock.login_flow().start("you@example.com").await?;
    Ok((pending, login_fixture_token()))
}

// ===================== LoginFlow::start =====================

#[tokio::test]
async fn login_flow_start_posts_expected_body_and_pending_login_holds_the_user_token()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(body_json(json!({ "login": "you@example.com" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let pending = mock.login_flow().start("you@example.com").await?;

    // `PendingLogin` keeps its login token private (by design, see `src/auth/flow.rs`'s module
    // docs); the only way to prove it holds `fixtures/login.json`'s `data.user_token` from an
    // external test crate is to observe the *next* request it issues carrying it as
    // `Cookie: s=<token>`.
    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(session_cookie_for(&login_fixture_token()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    pending.resend().await?;
    Ok(())
}

#[tokio::test]
async fn login_flow_start_with_a_phone_number_sends_an_identical_request_shape()
-> anyhow::Result<()> {
    // The server (and this client) never distinguishes an email from a phone number
    // (`src/auth/flow.rs` docs, `auth.py:87`); `+15555550100` is the NANP fictional-number
    // range reserved for exactly this purpose (never a real subscriber).
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(body_json(json!({ "login": "+15555550100" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    mock.login_flow().start("+15555550100").await?;
    Ok(())
}

#[tokio::test]
async fn login_flow_start_missing_user_token_is_authentication_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({ "meta": { "code": 200 }, "data": {} }).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a response with no data.user_token at all must fail");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
    );
    Ok(())
}

#[tokio::test]
async fn login_flow_start_null_user_token_is_authentication_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({ "meta": { "code": 200 }, "data": { "user_token": null } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a null data.user_token must fail");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
    );
    Ok(())
}

#[tokio::test]
async fn login_flow_start_empty_user_token_is_authentication_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({ "meta": { "code": 200 }, "data": { "user_token": "" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("an empty-string data.user_token must fail");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
    );
    Ok(())
}

// ===================== PendingLogin::resend =====================

#[tokio::test]
async fn pending_login_resend_posts_empty_body_with_login_token_cookie() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(session_cookie_for(&login_token))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let envelope = pending.resend().await?;
    assert_eq!(envelope.into_value(), json!({}));
    Ok(())
}

// ===================== PendingLogin::verify =====================

#[tokio::test]
async fn pending_login_verify_posts_expected_body_with_login_token_cookie() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .and(session_cookie_for(&login_token))
        .and(body_json(json!({ "code": "123456" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert!(session.is_valid());
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_without_set_cookie_uses_the_login_token_d16_branch_a()
-> anyhow::Result<()> {
    // *** THE ONE OPEN QUESTION IN PHASE 1 (rust-port-plan.md §7.2, decision D-16) ***
    // Branch (a): the server sends no fresh `Set-Cookie` on `login/verify`, so the resulting
    // `Session`'s token falls back to the same login token `LoginFlow::start` obtained
    // (`resolve_session_token`, `src/auth/flow.rs`) — this is Python's only *observable*
    // behaviour, since `eero-api` never reads `Set-Cookie` at all. See the sibling
    // `..._d16_branch_b` test below for the other branch; live capture decides which one the
    // real server actually exercises.
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert_eq!(session.token().expose_secret(), login_token);
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_with_a_fresh_set_cookie_uses_the_new_token_d16_branch_b()
-> anyhow::Result<()> {
    // *** THE ONE OPEN QUESTION IN PHASE 1 (rust-port-plan.md §7.2, decision D-16) ***
    // Branch (b): the server sends a fresh `Set-Cookie: s=...` on `login/verify` (an observation
    // independently made by `erikh/eero` per D-16 — nothing is adopted from it beyond the
    // observation itself). `resolve_session_token` (`src/auth/flow.rs`) prefers this fresh
    // cookie over the login token whenever the server actually sends one.
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("verify.json"))
                .append_header("set-cookie", "s=fresh-session-token"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert_eq!(session.token().expose_secret(), "fresh-session-token");
    assert_ne!(session.token().expose_secret(), login_token);
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_produces_a_session_with_a_thirty_day_expiry() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, _login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert!(session.is_valid());

    let expiry = session.expiry().expect("verify() always sets an expiry");
    let lifetime_days = u64::try_from(SESSION_LIFETIME_DAYS)
        .expect("SESSION_LIFETIME_DAYS is a small positive constant");
    let expected = SystemTime::now() + Duration::from_secs(lifetime_days * 24 * 3600);
    let drift = expiry
        .duration_since(expected)
        .unwrap_or_else(|err| err.duration());
    assert!(
        drift < Duration::from_secs(5),
        "expiry drifted by {drift:?}"
    );
    Ok(())
}

// ===================== AuthApi::logout =====================

#[tokio::test]
async fn auth_api_logout_posts_empty_body_and_clears_local_session_and_store() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    auth.logout().await?;

    assert!(!auth.is_authenticated());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_429_surfaces_rate_limit_but_still_clears_session_and_store()
-> anyhow::Result<()> {
    // *** The logout ruling ***: a deliberate divergence from `eero-api` (see `AuthApi::logout`'s
    // doc comment in `src/auth/mod.rs`). Python's cleanup only runs for a 401 or a generic `Api`
    // outcome — never for a 429 or a network failure — contradicting its own "Always clear local
    // credentials regardless of API response" comment. `rusteero` always clears, on every
    // outcome; this test pins that choice so it cannot silently regress back to Python's gap.
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let err = auth
        .logout()
        .await
        .expect_err("a 429 must surface, not be swallowed");
    assert!(matches!(err, Error::RateLimit { .. }));

    assert!(!auth.is_authenticated());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_500_surfaces_api_error_but_still_clears_session_and_store()
-> anyhow::Result<()> {
    // Same ruling as the 429 case above, for a generic server error.
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let err = auth
        .logout()
        .await
        .expect_err("a 500 must surface, not be swallowed");
    assert!(matches!(err, Error::Api { status: 500, .. }));

    assert!(!auth.is_authenticated());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

// ===================== AuthApi::set_session_token =====================

#[tokio::test]
async fn auth_api_set_session_token_empty_is_validation_error_with_no_requests() {
    let mock = MockEero::start().await;
    // No `Mock` registered: if `set_session_token` ever reached the network, a request would
    // hit an unconfigured wiremock server (a 404, not the expected local validation error) — the
    // same idiom `harness_smoke.rs`'s `transport_anonymous_...` test uses to prove zero requests.
    let auth = AuthApi::new(mock.transport_anonymous());

    let err = auth
        .set_session_token("")
        .expect_err("an empty token must be rejected before any request");
    assert!(matches!(
        err,
        Error::Validation { ref field, ref message, .. }
            if field == "token" && message == "must be a non-empty string"
    ));
}

#[tokio::test]
async fn auth_api_set_session_token_installs_a_session_and_persists_it_to_the_store()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        .base_url(mock.uri())
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);
    assert!(!auth.is_authenticated());

    auth.set_session_token("t")?;

    assert!(auth.is_authenticated());
    let persisted = store.load()?;
    assert_eq!(persisted.token().expose_secret(), "t");
    Ok(())
}

#[tokio::test]
async fn auth_api_set_session_token_preserves_an_existing_refresh_token() -> anyhow::Result<()> {
    // Python parity (`auth.py:411-413`, `the auth behaviour notes:315-317`):
    // `set_session_token()` only ever assigns `session_id`/`session_expiry`, never touching
    // `refresh_token` — so a refresh token already held by the current session must survive.
    let mock = MockEero::start().await;
    let initial = Session::from_json(
        r#"{"session_id":"old-token","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
    )?;
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(initial))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    auth.set_session_token("new-token")?;

    let session = auth.session().expect("session installed");
    assert_eq!(session.token().expose_secret(), "new-token");
    assert_eq!(
        session
            .refresh_token()
            .expect("refresh token preserved")
            .expose_secret(),
        "rt-1"
    );
    Ok(())
}

// ===================== AuthApi::clear_session_token / clear_auth_data =====================

#[tokio::test]
async fn auth_api_clear_session_token_nulls_token_but_keeps_refresh_token() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let initial = Session::from_json(
        r#"{"session_id":"tok","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
    )?;
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(initial))
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    auth.clear_session_token()?;

    assert!(!auth.is_authenticated());
    let session = auth
        .session()
        .expect("clear_session_token keeps a session value, just emptied");
    assert!(session.token().expose_secret().is_empty());
    assert_eq!(
        session
            .refresh_token()
            .expect("refresh token preserved in memory")
            .expose_secret(),
        "rt-1"
    );

    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    assert_eq!(
        persisted
            .refresh_token()
            .expect("refresh token preserved in the store")
            .expose_secret(),
        "rt-1"
    );
    Ok(())
}

#[tokio::test]
async fn auth_api_clear_auth_data_clears_everything_and_removes_the_store_entry()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let initial = Session::from_json(
        r#"{"session_id":"tok","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
    )?;
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(initial))
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    auth.clear_auth_data()?;

    assert!(!auth.is_authenticated());
    // The real behavioural distinction from `clear_session_token` above, asserted explicitly so
    // it cannot be collapsed by accident: `clear_auth_data` clears the in-memory session to
    // `None` entirely, not merely to an emptied `Session` value.
    assert!(auth.session().is_none());

    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    assert!(
        persisted.refresh_token().is_none(),
        "clear_auth_data clears the refresh token too, unlike clear_session_token"
    );
    Ok(())
}

// ===================== AuthApi::ensure_authenticated =====================

#[tokio::test]
async fn auth_api_ensure_authenticated_with_no_session_is_authentication_error_with_no_requests() {
    let mock = MockEero::start().await;
    // No `Mock` registered — see `auth_api_set_session_token_empty_is_validation_error_...`
    // above for why this is the proof that zero requests were made.
    let auth = AuthApi::new(mock.transport_anonymous());

    let err = auth
        .ensure_authenticated()
        .await
        .expect_err("no session configured means this must fail locally");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Not authenticated")
    );
}

// ===================== Session loaded from a store with a past expiry =====================

#[tokio::test]
async fn auth_api_session_loaded_from_store_with_past_expiry_is_not_authenticated()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let expired = Session::from_json(
        r#"{"session_id":"tok","refresh_token":null,"session_expiry":"2000-01-01T00:00:00"}"#,
    )?;
    store.save(&expired)?;
    let loaded = store.load()?;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(loaded))
        .store(Some(store))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    assert!(!auth.is_authenticated());
    Ok(())
}
