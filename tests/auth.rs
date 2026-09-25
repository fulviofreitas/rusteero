//! v8.0.4 auth suite: the interactive login handshake (`LoginFlow`/`PendingLogin`) and the
//! network-facing half of `AuthApi` (`login`/`resend`/`verify`, `logout`, the local
//! `set_session_token`/`clear_*` scopes), all against a local wiremock server per the crate's
//! testing conventions.

mod common;

use std::sync::Arc;

use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header_for};
use rusteero::auth::flow::PendingLogin;
use rusteero::auth::{AuthApi, Session};
use rusteero::error::Error;
use rusteero::storage::{CredentialStore, MemoryStore};
use rusteero::transport::Transport;

/// The `data.user_token` value baked into `fixtures/login.json`.
fn login_fixture_token() -> String {
    fixture_json("login.json")["data"]["user_token"]
        .as_str()
        .expect("fixtures/login.json always carries a string data.user_token")
        .to_owned()
}

/// Mounts the shared `/2.2/login` mock (responding with `fixtures/login.json`) and completes
/// `LoginFlow::start`, handing back the resulting `PendingLogin` together with the login token
/// `fixtures/login.json` carries.
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

// ===================== LoginFlow::start: form encoding =====================

#[tokio::test]
async fn login_flow_start_posts_a_form_encoded_body_unauthenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("login=you%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let pending = mock.login_flow().start("you@example.com").await?;

    // `PendingLogin` keeps its login token private; the only way to prove it holds
    // `fixtures/login.json`'s `data.user_token` from an external test crate is to observe the
    // *next* request it issues carrying it as `X-User-Token`.
    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(user_token_header_for(&login_fixture_token()))
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
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(body_string("login=%2B15555550100"))
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

#[tokio::test]
async fn login_flow_start_a_400_validation_error_is_wrapped_as_authentication() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"error.form.email.malformed"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("not-an-email")
        .await
        .expect_err("a server-rejected identifier must fail");
    match err {
        Error::Authentication { message, .. } => {
            assert!(message.starts_with("Login failed: "));
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn login_flow_start_a_401_propagates_unwrapped() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a 401 during login must still surface as Error::Authentication");
    match err {
        Error::Authentication { message, .. } => {
            assert!(
                !message.starts_with("Login failed: "),
                "a real 401 must not be re-wrapped with the Login-failed prefix: {message}"
            );
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

// ===================== PendingLogin::resend: JSON body (the one asymmetric case) =====================

#[tokio::test]
async fn pending_login_resend_posts_json_empty_body_with_the_login_token() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(user_token_header_for(&login_token))
        .and(header("content-type", "application/json"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let envelope = pending.resend().await?;
    assert_eq!(envelope.into_value(), json!({}));
    Ok(())
}

// ===================== PendingLogin::verify: form encoding =====================

#[tokio::test]
async fn pending_login_verify_posts_a_form_encoded_body_with_the_login_token() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .and(user_token_header_for(&login_token))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("code=123456"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert!(session.is_valid());
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_response_is_discarded_the_login_token_becomes_the_session()
-> anyhow::Result<()> {
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
async fn pending_login_verify_a_401_propagates_unwrapped() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, _login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.verification.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = pending
        .verify("000000")
        .await
        .expect_err("a wrong code must surface as Error::Authentication");
    match err {
        Error::Authentication { message, .. } => {
            assert!(
                !message.starts_with("Verification failed: "),
                "a real 401 must not be re-wrapped: {message}"
            );
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

// ===================== AuthApi::set_session_token =====================

#[tokio::test]
async fn auth_api_set_session_token_empty_is_validation_error_with_no_requests() {
    let mock = MockEero::start().await;
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

// ===================== AuthApi::clear_session_token / clear_auth_data: now identical =====================

async fn assert_clear_scope_clears_everything(
    clear: fn(&AuthApi) -> Result<(), Error>,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token("tok")))
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    clear(&auth)?;

    assert!(!auth.is_authenticated());
    assert!(auth.session().is_none());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_clear_session_token_clears_everything() -> anyhow::Result<()> {
    assert_clear_scope_clears_everything(AuthApi::clear_session_token).await
}

#[tokio::test]
async fn auth_api_clear_auth_data_clears_everything() -> anyhow::Result<()> {
    assert_clear_scope_clears_everything(AuthApi::clear_auth_data).await
}

// ===================== AuthApi::ensure_authenticated =====================

#[tokio::test]
async fn auth_api_ensure_authenticated_with_no_session_is_authentication_error_with_no_requests() {
    let mock = MockEero::start().await;
    let auth = AuthApi::new(mock.transport_anonymous());

    let err = auth
        .ensure_authenticated()
        .await
        .expect_err("no session configured means this must fail locally");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Not authenticated")
    );
}

// ===================== AuthApi::logout =====================

#[tokio::test]
async fn auth_api_logout_when_not_authenticated_returns_false_with_no_network_call() {
    let mock = MockEero::start().await;
    // No `Mock` registered at all: if `logout` ever attempted a network call, wiremock would
    // panic with "no mock matched" rather than let this reach the assertion below.
    let auth = AuthApi::new(mock.transport_anonymous());

    let result = auth
        .logout()
        .await
        .expect("no session configured: the local guard fires, no network call attempted");
    assert!(!result);
}

#[tokio::test]
async fn auth_api_logout_posts_a_form_body_with_the_cookie_shaped_field_and_clears_session()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(format!("Cookie=s%3D{TEST_TOKEN}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth.logout().await?;
    assert!(result);

    assert!(!auth.is_authenticated());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_401_and_still_clears_session_and_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.expired"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth
        .logout()
        .await
        .expect("v8.0.4 logout never propagates a network/API error");
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_429_and_still_clears_session_and_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth.logout().await?;
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_network_error_and_still_clears_session_and_store()
-> anyhow::Result<()> {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        // No listener at all on this port.
        .base_url("http://127.0.0.1:1")
        .session(Some(Session::from_token(TEST_TOKEN)))
        .store(Some(Arc::clone(&store)))
        .build()?;
    let auth = AuthApi::new(transport);

    let result = auth
        .logout()
        .await
        .expect("a network failure during logout must never propagate");
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}
