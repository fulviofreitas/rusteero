//! HTTP integration tests for `PasswordApi` (`src/endpoints/password.rs`).
//!
//! `PasswordApi::get_password` has no fixture under `tests/fixtures/`, so its test builds a
//! small, obviously-synthetic `{"meta": …, "data": …}` body inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_password.py::TestPasswordAPIGetPassword::test_get_password_returns_raw_response`).
//! No real MACs, serials, IPs or names appear in any fixture here.
//!
//! Per the crate's testing conventions, every test pins the exact verb, path and session cookie
//! against a local `wiremock` server, and asserts the returned `Envelope` is byte-identical to
//! the body served via `into_value()` — the raw wire payload is the contract, never a reshaped
//! view of it. A dedicated test additionally pins the security guarantee that matters most for
//! `PasswordApi::get_password`: its response body (the network's Wi-Fi password) never appears
//! in the `Envelope`'s `Debug` output.

mod common;

use std::sync::Arc;

use rusteero::endpoints::password::PasswordApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

// ===================== get_password =====================

#[tokio::test]
async fn get_password_hits_v22_path_with_session_cookie_and_matches_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"password": "correct-horse-battery-staple", "ssid": "SynthNet"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = PasswordApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_password("network-0001").await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

/// Pins the guarantee `PasswordApi::get_password`'s doc comment promises: even though
/// `Envelope` carries the network's Wi-Fi password verbatim in `data`, its `Debug` impl
/// (`src/envelope.rs`) never renders the payload — only a `meta.code`/`data` "kind" summary —
/// so an incidental `{:?}` on the returned envelope can never leak the password into a log line.
#[tokio::test]
async fn get_password_debug_output_never_contains_the_password() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let secret_password = "correct-horse-battery-staple";
    let body = json!({
        "meta": {"code": 200},
        "data": {"password": secret_password, "ssid": "SynthNet"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/password"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = PasswordApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let env = api.get_password("network-0001").await?;

    let rendered = format!("{env:?}");
    assert!(!rendered.contains(secret_password));
    Ok(())
}
