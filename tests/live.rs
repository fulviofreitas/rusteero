//! Live smoke tests against the real Eero cloud API.
//!
//! Every test in this file is `#[ignore]`, and additionally checks
//! [`live_credentials`]/its callers at runtime before doing anything network-visible, so a plain
//! `cargo test` (no flags) never touches the network, and even an explicit
//! `cargo test --test live -- --ignored` run with the required environment unset skips cleanly
//! (prints a message, returns `Ok(())`) instead of failing. Per `.claude/rules/testing.md`, run
//! deliberately, by a human, never in CI:
//!
//! ```text
//! RUSTEERO_LIVE=1 RUSTEERO_SESSION_TOKEN=... cargo test --test live -- --ignored
//! ```
//!
//! `RUSTEERO_SESSION_TOKEN` is a session token already obtained out of band (e.g. from a prior
//! interactive login) — this file's tests only ever perform read-only calls with it, except for
//! [`live_verify_sets_a_fresh_session_cookie_open_question_d16`], which is explicitly about
//! performing a *fresh* login and is gated on its own additional environment variables (see that
//! test's doc comment).

use rusteero::auth::AuthApi;
use rusteero::auth::Session;
use rusteero::auth::flow::LoginFlow;
use rusteero::routes::ACCOUNT;
use rusteero::transport::Transport;

/// Returns `Some(token)` only when both `RUSTEERO_LIVE=1` and a non-empty `RUSTEERO_SESSION_TOKEN`
/// are set in the environment.
///
/// This is the runtime half of this file's double gate: `#[ignore]` keeps a plain `cargo test`
/// from ever selecting these tests at all; this check keeps a bare
/// `cargo test --test live -- --ignored` (environment not configured) from attempting any
/// network call in the first place, printing a skip message and returning `None` instead.
fn live_credentials() -> Option<String> {
    let live = std::env::var("RUSTEERO_LIVE").ok()?;
    if live != "1" {
        return None;
    }
    let token = std::env::var("RUSTEERO_SESSION_TOKEN").ok()?;
    if token.is_empty() {
        return None;
    }
    Some(token)
}

// ===================== read-only handshake smoke test =====================

/// Starts (but does not complete) the interactive login handshake against the real Eero cloud
/// API, proving [`LoginFlow::start`] round-trips against the live server.
///
/// This only *starts* the handshake — it never calls `PendingLogin::verify`, so it never mutates
/// which session is currently active on the account. It does, however, cause the server to send
/// a fresh one-time code to `RUSTEERO_LOGIN_IDENTIFIER` as a side effect of starting the
/// handshake; that email/SMS can simply be ignored (or reused for
/// [`live_verify_sets_a_fresh_session_cookie_open_question_d16`], run separately).
#[tokio::test]
#[ignore = "hits the real Eero cloud API; see this file's module docs for how to run it"]
async fn live_login_start_returns_a_pending_login() {
    let Some(_gate) = live_credentials() else {
        eprintln!(
            "skipping live_login_start_returns_a_pending_login: set RUSTEERO_LIVE=1 and \
             RUSTEERO_SESSION_TOKEN to run live tests"
        );
        return;
    };
    let Ok(identifier) = std::env::var("RUSTEERO_LOGIN_IDENTIFIER") else {
        eprintln!(
            "skipping live_login_start_returns_a_pending_login: set \
             RUSTEERO_LOGIN_IDENTIFIER (the account's email or phone number) to run this test"
        );
        return;
    };

    let flow = LoginFlow::new(None).expect("builds against the real Eero cloud hosts");
    let pending = flow
        .start(&identifier)
        .await
        .expect("login should start and return a pending login");
    // `PendingLogin`'s `Debug` impl redacts the token unconditionally; safe to print.
    println!("login started: {pending:?}");
}

// ===================== get_account smoke test =====================

/// Fetches `/account` with an already-obtained session token, proving the transport, session
/// cookie attachment, and status-to-envelope mapping all work end to end against the live
/// server.
#[tokio::test]
#[ignore = "hits the real Eero cloud API; see this file's module docs for how to run it"]
async fn live_get_account_smoke() {
    let Some(token) = live_credentials() else {
        eprintln!(
            "skipping live_get_account_smoke: set RUSTEERO_LIVE=1 and RUSTEERO_SESSION_TOKEN to \
             run live tests"
        );
        return;
    };

    let transport = Transport::builder()
        .session(Some(Session::from_token(token)))
        .build()
        .expect("builds against the real Eero cloud hosts");
    let auth = AuthApi::new(transport);
    assert!(
        auth.is_authenticated(),
        "RUSTEERO_SESSION_TOKEN did not produce a locally-valid session"
    );

    let envelope = auth
        .transport()
        .send(&ACCOUNT, &[], None)
        .await
        .expect("GET /account should succeed with a valid session token");
    assert_eq!(envelope.meta().code, Some(200));
    // Deliberately does not print `envelope`'s contents: an account response can carry the
    // account holder's email/name, and this crate's own `Envelope::Debug` impl only ever prints
    // a `meta.code`/`data` kind summary for exactly this reason.
    println!("GET /account: meta.code = {:?}", envelope.meta().code);
}

// ===================== the verify-Set-Cookie open question (rust-port-plan.md §7.2, D-16) =====================

/// Diagnostic-only test to settle the one open question this port still has: does a real
/// `login/verify` response ever carry a fresh `Set-Cookie: s=...` header?
///
/// `eero-api` cannot answer this either way — it never reads `Set-Cookie` explicitly (it relies
/// on aiohttp's implicit cookie jar) — and `rusteero` has no implicit jar to observe it
/// silently, so [`PendingLogin::verify`](rusteero::auth::flow::PendingLogin::verify) falls back
/// to the login token whenever no fresh cookie is present, which is indistinguishable from
/// "there never was one" from outside the crate. This test therefore does **not** go through
/// `rusteero`'s own `LoginFlow`/`PendingLogin` at all: it makes the login and verify calls with a
/// bare `reqwest::Client` instead, purely so it can inspect the raw `Set-Cookie` response header
/// this crate's own transport deliberately never logs (crate security rule: never log headers,
/// cookies, or bodies).
///
/// # Running this test
///
/// Requires `RUSTEERO_LIVE=1`, `RUSTEERO_SESSION_TOKEN` (the blanket gate every test in this
/// file shares), plus `RUSTEERO_LOGIN_IDENTIFIER` and a **freshly requested** `RUSTEERO_LOGIN_CODE`
/// (one-time codes are single-use and short-lived, so this cannot be automated in CI — a human
/// must request a code via [`live_login_start_returns_a_pending_login`] or the mobile/web app
/// immediately before running this test with the code it received).
///
/// This test **mutates the live account's active session** (a real login/verify rotates it):
/// update `RUSTEERO_SESSION_TOKEN` afterward if other live tests depend on the old one.
///
/// There is no hard assertion either way; the answer is printed for a human to copy into
/// `rust-port-plan.md` §7.2 to close out decision D-16.
#[tokio::test]
#[ignore = "hits the real Eero cloud API and rotates the account's session; see this test's doc comment"]
async fn live_verify_sets_a_fresh_session_cookie_open_question_d16() {
    let Some(_gate) = live_credentials() else {
        eprintln!(
            "skipping live_verify_sets_a_fresh_session_cookie_open_question_d16: set \
             RUSTEERO_LIVE=1 and RUSTEERO_SESSION_TOKEN to run live tests"
        );
        return;
    };
    let (Ok(identifier), Ok(code)) = (
        std::env::var("RUSTEERO_LOGIN_IDENTIFIER"),
        std::env::var("RUSTEERO_LOGIN_CODE"),
    ) else {
        eprintln!(
            "skipping live_verify_sets_a_fresh_session_cookie_open_question_d16: set \
             RUSTEERO_LOGIN_IDENTIFIER and RUSTEERO_LOGIN_CODE (a freshly requested one-time \
             code) to run this test"
        );
        return;
    };

    // No `.cookie_store(true)` (the crate's `cookies` cargo feature is not enabled — this crate
    // never wants an implicit jar, see `src/transport.rs`'s module docs), so `client` never
    // stores or forwards the `Set-Cookie` this test needs to inspect directly.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client builds");

    let login_response = client
        .post("https://api-user.e2ro.com/2.2/login")
        .json(&serde_json::json!({ "login": identifier }))
        .send()
        .await
        .expect("login request should succeed");
    let login_body: serde_json::Value =
        login_response.json().await.expect("login response is JSON");
    let login_token = login_body["data"]["user_token"]
        .as_str()
        .expect("login response carries a non-empty data.user_token")
        .to_owned();

    let verify_response = client
        .post("https://api-user.e2ro.com/2.2/login/verify")
        .header("cookie", format!("s={login_token}"))
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .expect("verify request should succeed with a freshly requested code");

    let fresh_cookie_present = verify_response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|raw| raw.trim_start().starts_with("s="));

    println!(
        "D-16: login/verify {} send a fresh `s` Set-Cookie header",
        if fresh_cookie_present {
            "DID"
        } else {
            "did NOT"
        }
    );
}
