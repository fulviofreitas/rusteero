//! `AccountApi` suite (`src/endpoints/account.rs`, new in v8.0.0) against a local `wiremock`
//! server per the crate's testing conventions.
//!
//! `AccountApi` has no fixture under `tests/fixtures/`, so every response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit tests
//! (`tests/api/test_account.py`). No real emails/phone numbers/codes appear anywhere in this
//! file — every value is an obvious placeholder.

mod common;

use std::sync::Arc;

use rusteero::endpoints::account::AccountApi;
use serde_json::json;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn account_api(mock: &MockEero) -> AccountApi {
    AccountApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

// ===================== set_name =====================

#[tokio::test]
async fn set_name_sends_a_form_encoded_put() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/name"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("name=Test-User"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).set_name("Test-User").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ===================== set_email / verify_email =====================

#[tokio::test]
async fn set_email_sends_a_form_encoded_put() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/email"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("email=user%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).set_email("user@example.com").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn verify_email_sends_a_form_encoded_post() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/email/verify"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("code=123456"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).verify_email("123456").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ===================== set_phone / verify_phone =====================

#[tokio::test]
async fn set_phone_sends_a_form_encoded_put() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/phone"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("phone=%2B15555550100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).set_phone("+15555550100").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn verify_phone_sends_a_form_encoded_post() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/phone/verify"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("code=654321"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).verify_phone("654321").await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ===================== set_consents =====================

#[tokio::test]
async fn set_consents_true_sends_marketing_emails_true() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/consents"))
        .and(session_cookie())
        .and(body_string("marketing_emails=true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).set_consents(true).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_consents_false_sends_marketing_emails_false() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/consents"))
        .and(session_cookie())
        .and(body_string("marketing_emails=false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).set_consents(false).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

// ===================== get_sms_countries =====================

#[tokio::test]
async fn get_sms_countries_returns_the_raw_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"countries": ["US", "CA"]} });
    Mock::given(method("GET"))
        .and(path("/2.2/countries/sms"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = account_api(&mock).get_sms_countries().await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== not authenticated =====================

#[tokio::test]
async fn set_name_not_authenticated_returns_authentication_error_without_a_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/name"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = AccountApi::new(std::sync::Arc::new(mock.transport_anonymous()));
    let err = api.set_name("Test-User").await.unwrap_err();
    assert!(err.is_auth_error());
    Ok(())
}
