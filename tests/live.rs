//! Live smoke tests against the real Eero cloud API.
//!
//! Every test in this file is `#[ignore]`, and additionally checks
//! [`live_credentials`]/its callers at runtime before doing anything network-visible, so a plain
//! `cargo test` (no flags) never touches the network, and even an explicit
//! `cargo test --test live -- --ignored` run with the required environment unset skips cleanly
//! (prints a message, returns `Ok(())`) instead of failing. Per the crate's testing conventions, run
//! deliberately, by a human, never in CI:
//!
//! ```text
//! RUSTEERO_LIVE=1 RUSTEERO_SESSION_TOKEN=... cargo test --test live -- --ignored
//! ```
//!
//! `RUSTEERO_SESSION_TOKEN` is a session token already obtained out of band (e.g. from a prior
//! interactive login) — this file's tests only ever perform read-only calls with it.
//!
//! The former D-16 diagnostic (`login/verify`'s `Set-Cookie` behaviour) is gone: `v8.0.4` has no
//! `Set-Cookie` reader anywhere in `base.py`/`auth.py`, and the login token is unconditionally the
//! session token (see `crate::auth::flow::PendingLogin::verify`'s docs), so there is no longer an
//! open question for a live capture to settle.

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

// ===================== read-only endpoint sweep =====================

/// Hits every read-only endpoint once against the live API and reports, per endpoint, the HTTP
/// status this crate mapped and the top-level keys of `data`.
///
/// This is the only thing in the suite that validates the *paths themselves* against the real
/// server. Everything else in `tests/` proves that a request matches what a wiremock mock was
/// told to expect — which is a statement about this crate's own consistency, not about Eero.
/// A route constant with a wrong path would pass every offline test and fail only here.
///
/// Read-only by construction: every entry below is a `GET`, and the list is written out
/// explicitly rather than derived, so no future mutation route can be swept in by accident.
///
/// It never prints a response body. Several of these responses carry the Wi-Fi password, device
/// MACs, or the account holder's name and email; only `meta.code` and the *names* of the
/// top-level `data` keys are printed. It also does not assert a 200: an account without Eero Plus
/// legitimately 404s on some resources, and the point is to observe reality, not to impose an
/// expectation.
#[tokio::test]
#[ignore = "hits the real Eero cloud API; see this file's module docs for how to run it"]
async fn live_read_only_endpoint_sweep() {
    let Some(token) = live_credentials() else {
        eprintln!(
            "skipping live_read_only_endpoint_sweep: set RUSTEERO_LIVE=1 and \
             RUSTEERO_SESSION_TOKEN to run live tests"
        );
        return;
    };

    let client = rusteero::Client::builder()
        .session(Some(Session::from_token(token)))
        .build()
        .await
        .expect("builds against the real Eero cloud hosts");

    // Resolve a network id the same way a real caller would, exercising get_networks and the
    // auto-discovery path against live data.
    let networks = client
        .get_networks(false)
        .await
        .expect("GET /networks should succeed with a valid session token");
    println!(
        "networks: meta.code={:?} {}",
        networks.meta().code,
        summarise(networks.data())
    );

    let nid = client
        .preferred_network_id()
        .expect("get_networks should have set a preferred network id as a side effect");
    println!("resolved network id: <{} chars>", nid.len());

    let mut ok = 0_u32;
    let mut failed: Vec<(&str, String)> = Vec::new();

    macro_rules! probe {
        ($label:literal, $call:expr) => {
            match $call.await {
                Ok(env) => {
                    ok += 1;
                    println!(
                        "  {:<22} {:?}  {}",
                        $label,
                        env.meta().code,
                        summarise(env.data())
                    );
                }
                Err(err) => {
                    // Record the variant, not the message: an error message can embed a
                    // (sanitised) response body, and this output is meant to be pasteable.
                    let variant = match &err {
                        rusteero::Error::Api { status, .. } => format!("Api {status}"),
                        other => format!("{}", ErrorKind(other)),
                    };
                    println!("  {:<22} ERR  {variant}", $label);
                    failed.push(($label, variant));
                }
            }
        };
    }

    let n = Some(nid.as_str());
    probe!("account", client.get_account(false));
    probe!("network", client.get_network(n, false));
    probe!("eeros", client.get_eeros(n, false));
    probe!("devices", client.get_devices(n, false));
    probe!("profiles", client.get_profiles(n, false));
    probe!("settings", client.get_settings(n));
    probe!("dns_settings", client.get_dns_settings(n));
    probe!("security_settings", client.get_security_settings(n));
    probe!("sqm_settings", client.get_sqm_settings(n));
    probe!("password", client.get_password(n));
    probe!("blacklist", client.get_blacklist(n));
    probe!("reservations", client.get_reservations(n));
    probe!("forwards", client.get_forwards(n));
    probe!("routing", client.get_routing(n));
    probe!("thread", client.get_thread(n));
    probe!("updates", client.get_updates(n));
    probe!("ac_compat", client.get_ac_compat(n));
    probe!("support", client.get_support(n));
    probe!("diagnostics", client.get_diagnostics(n));
    probe!("transfer_stats", client.get_transfer_stats(n, None));
    probe!("burst_reporters", client.get_burst_reporters(n));
    probe!("backup_network", client.get_backup_network(n));
    probe!("backup_status", client.get_backup_status(n));
    probe!("ouicheck", client.get_ouicheck(n));
    probe!("premium_status", client.get_premium_status(n));

    println!(
        "\nlive sweep: {ok} endpoints returned an envelope, {} errored",
        failed.len()
    );
    for (label, variant) in &failed {
        println!("  errored: {label} -> {variant}");
    }

    // The sweep is diagnostic; it fails only if the *client-level* calls that everything else
    // depends on are broken. A 404 on, say, `backup` for an account without that feature is
    // information, not a defect.
    assert!(
        ok >= 5,
        "expected at least the core read-only endpoints to answer"
    );
}

/// Renders the *shape* of a `data` payload — never its contents.
fn summarise(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("object keys: [{}]", keys.join(", "))
        }
        serde_json::Value::Array(items) => format!("array of {}", items.len()),
        serde_json::Value::Null => "null".to_owned(),
        other => format!("{} scalar", kind_of(other)),
    }
}

fn kind_of(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Prints an `Error`'s variant name without its payload, so a sweep transcript can be pasted
/// into an issue without leaking a sanitised-but-still-detailed server message.
struct ErrorKind<'a>(&'a rusteero::Error);

impl std::fmt::Display for ErrorKind<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.0 {
            rusteero::Error::Authentication { .. } => "Authentication",
            rusteero::Error::RateLimit { .. } => "RateLimit",
            rusteero::Error::Network(_) => "Network",
            rusteero::Error::Api { .. } => "Api",
            rusteero::Error::Timeout => "Timeout",
            rusteero::Error::NotFound { .. } => "NotFound",
            rusteero::Error::PremiumRequired { .. } => "PremiumRequired",
            rusteero::Error::FeatureUnavailable { .. } => "FeatureUnavailable",
            rusteero::Error::Validation { .. } => "Validation",
            rusteero::Error::MissingNetworkId => "MissingNetworkId",
            rusteero::Error::Storage(_) => "Storage",
            rusteero::Error::Json(_) => "Json",
            _ => "Other",
        };
        f.write_str(name)
    }
}
